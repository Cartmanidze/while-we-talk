use crate::{core::{AnswerState, Conversation}, rooms::{Room, RoomRecoveryResult, RoomStore}};
use std::{sync::Arc, time::Instant};

pub struct RoomSession {
    pub store: Arc<RoomStore>,
    pub room: Option<Room>,
    pub conversation: Conversation,
}

impl RoomSession {
    pub fn trash(&mut self,id:&str)->Result<(),String> {
        if !crate::rooms::valid_id(id) { return Err("Некорректный идентификатор комнаты".into()); }
        let snapshot=self.room.as_ref().map(|room| {
            let mut snapshot=room.clone();snapshot.entries=self.conversation.entries();snapshot.refresh_title();snapshot
        });
        if let Some(snapshot)=&snapshot { self.store.save(snapshot)?; }
        self.store.trash_room(id)?;
        if self.room.as_ref().is_some_and(|room|room.id==id) {
            self.room=None;self.conversation=Conversation::default();
        } else { self.room=snapshot; }
        Ok(())
    }
    pub fn restore(&mut self,id:&str)->Result<(),String> { self.store.restore_room(id) }
    pub fn new(store: RoomStore) -> Self {
        Self { store: Arc::new(store), room: None, conversation: Conversation::default() }
    }
    fn activate(&mut self, room: Room) {
        self.conversation = Conversation::from_entries(room.entries.clone());
        self.room = Some(room);
    }
    pub fn create(&mut self) -> Result<(), String> {
        self.save()?;
        let room = self.store.create()?;
        self.activate(room);
        Ok(())
    }
    pub fn open(&mut self, id: &str) -> Result<(), String> {
        self.save()?;
        let room = self.store.load(id)?;
        self.activate(room);
        Ok(())
    }
    pub fn recover(&mut self, id:&str)->Result<RoomRecoveryResult,String> {
        let info=self.store.recovery_info(id)?;
        if info.primary_error.is_none() { return Err("Основной файл комнаты исправен; восстановление не требуется".into()); }
        if !info.available {
            return Err(format!("Нет исправной резервной копии для восстановления. {}",info.backup_error.or(info.primary_error).unwrap_or_default()));
        }
        let preserved_memory_path=if let Some(room)=self.room.as_ref().filter(|room|room.id==id) {
            let mut snapshot=room.clone();snapshot.entries=self.conversation.entries();snapshot.refresh_title();
            Some(self.store.preserve_unsaved(&snapshot)?)
        } else { self.save()?;None };
        let mut recovered=self.store.recover(id).map_err(|error|match &preserved_memory_path {
            Some(path)=>format!("{error}. Несохранённая история сохранена: {path}"),None=>error,
        })?;
        recovered.preserved_memory_path=preserved_memory_path;
        self.activate(recovered.room.clone());
        Ok(recovered)
    }
    pub fn save(&mut self) -> Result<(), String> {
        if let Some(room) = self.room.as_mut() {
            room.entries = self.conversation.entries();
            room.refresh_title();
            room.updated_at = chrono::Utc::now().to_rfc3339();
            // Keep unsaved text in memory on disk errors; switching rooms retries this write first.
            self.store.save(room)?;
        }
        Ok(())
    }
    pub fn add_utterance(&mut self, source: &str, text: &str, when: Instant) -> Result<bool, String> {
        if self.room.is_none() { return Err("Сначала создай или открой комнату".into()); }
        let added = self.conversation.add_utterance(source, text, when);
        if added { self.save()?; }
        Ok(added)
    }
    pub fn add_answer(&mut self, answer: AnswerState) -> Result<(), String> {
        let room = self.room.as_mut().ok_or("Комната не открыта")?;
        self.conversation.add_answer(&answer.text);
        room.answer = answer;
        self.save()
    }
    pub fn set_memory(&mut self, text: String, through: usize) -> Result<(), String> {
        let room = self.room.as_mut().ok_or("Комната не открыта")?;
        if through > self.conversation.entries().len() || (through > 0 && text.trim().is_empty()) {
            return Err("Изменились границы контекста комнаты".into());
        }
        room.memory = text; room.memory_through = through;
        self.save()
    }
    pub fn set_summary(&mut self, text: String, through: usize) -> Result<(), String> {
        let room = self.room.as_mut().ok_or("Комната не открыта")?;
        if through > self.conversation.entries().len() { return Err("Изменились границы итога комнаты".into()); }
        room.summary = text; room.summary_through = through;
        self.save()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn cleanup(path: std::path::PathBuf) {
        for entry in std::fs::read_dir(&path).unwrap() {
            let entry=entry.unwrap();
            if entry.file_name()=="trash"&&entry.file_type().unwrap().is_dir() {
                for file in std::fs::read_dir(entry.path()).unwrap() { std::fs::remove_file(file.unwrap().path()).unwrap(); }
                std::fs::remove_dir(entry.path()).unwrap();
            } else { std::fs::remove_file(entry.path()).unwrap(); }
        }
        std::fs::remove_dir(path).unwrap();
    }
    #[test]
    fn trash_clears_active_room_and_restore_does_not_open_it() {
        let path=std::env::temp_dir().join(format!("sobesednik-session-{:032x}",rand::random::<u128>()));
        let mut session=RoomSession::new(RoomStore::open(path.clone()).unwrap());session.create().unwrap();
        let id=session.room.as_ref().unwrap().id.clone();
        session.add_utterance("user","Полная история удаляемой комнаты",Instant::now()).unwrap();
        session.trash(&id).unwrap();assert!(session.room.is_none());assert!(session.conversation.entries().is_empty());
        assert_eq!(session.store.trash_list().unwrap().len(),1);
        session.restore(&id).unwrap();assert!(session.room.is_none());assert!(session.conversation.entries().is_empty());
        session.open(&id).unwrap();assert_eq!(session.conversation.entries()[0].text,"Полная история удаляемой комнаты");
        drop(session);cleanup(path);
    }
    #[test]
    fn failed_active_save_prevents_trash_and_preserves_ram() {
        let path=std::env::temp_dir().join(format!("sobesednik-session-{:032x}",rand::random::<u128>()));
        let mut session=RoomSession::new(RoomStore::open(path.clone()).unwrap());session.create().unwrap();
        let id=session.room.as_ref().unwrap().id.clone();let primary=path.join(format!("{id}.json"));
        std::fs::write(&primary,b"corrupt primary").unwrap();
        assert!(session.add_utterance("user","Новая реплика должна остаться в памяти",Instant::now()).is_err());
        let before=serde_json::to_vec(session.room.as_ref().unwrap()).unwrap();
        assert!(session.trash(&id).is_err());assert_eq!(serde_json::to_vec(session.room.as_ref().unwrap()).unwrap(),before);
        assert_eq!(session.conversation.entries()[0].text,"Новая реплика должна остаться в памяти");
        assert!(session.store.trash_list().unwrap().is_empty());assert_eq!(std::fs::read(primary).unwrap(),b"corrupt primary");
        drop(session);cleanup(path);
    }
    #[test]
    fn trashing_an_inactive_room_keeps_current_conversation_and_failed_marker_does_not_switch_it() {
        let path=std::env::temp_dir().join(format!("sobesednik-session-{:032x}",rand::random::<u128>()));
        let mut session=RoomSession::new(RoomStore::open(path.clone()).unwrap());session.create().unwrap();
        let first=session.room.as_ref().unwrap().id.clone();session.add_utterance("user","Первая комната",Instant::now()).unwrap();
        session.create().unwrap();let second=session.room.as_ref().unwrap().id.clone();session.add_utterance("user","Текущая комната",Instant::now()).unwrap();
        std::fs::write(path.join("trash"),b"not a directory").unwrap();
        let before=serde_json::to_vec(session.room.as_ref().unwrap()).unwrap();
        assert!(session.trash(&first).is_err());assert_eq!(serde_json::to_vec(session.room.as_ref().unwrap()).unwrap(),before);
        assert_eq!(session.conversation.entries()[0].text,"Текущая комната");
        std::fs::remove_file(path.join("trash")).unwrap();
        session.trash(&first).unwrap();assert_eq!(session.room.as_ref().unwrap().id,second);
        assert_eq!(session.conversation.entries()[0].text,"Текущая комната");
        assert_eq!(session.store.load(&second).unwrap().entries[0].text,"Текущая комната");
        drop(session);cleanup(path);
    }
    #[test]
    fn failed_save_keeps_text_and_prevents_room_switch() {
        let path = std::env::temp_dir().join(format!("sobesednik-session-{:032x}", rand::random::<u128>()));
        let mut session = RoomSession::new(RoomStore::open(path.clone()).unwrap());
        session.create().unwrap();
        let first = session.room.as_ref().unwrap().id.clone();
        std::fs::write(path.join(format!("{first}.json")), b"broken file").unwrap();
        assert!(session.add_utterance("user", "Текст не должен исчезнуть", Instant::now()).is_err());
        assert!(session.create().is_err());
        assert_eq!(session.room.as_ref().unwrap().id, first);
        assert_eq!(session.conversation.entries()[0].text, "Текст не должен исчезнуть");
        assert_eq!(std::fs::read(path.join(format!("{first}.json"))).unwrap(), b"broken file");
        drop(session);
        cleanup(path);
    }
    #[test]
    fn switching_rooms_and_reopening_keeps_separate_full_histories() {
        let path = std::env::temp_dir().join(format!("sobesednik-session-{:032x}", rand::random::<u128>()));
        let mut session = RoomSession::new(RoomStore::open(path.clone()).unwrap());
        assert!(session.add_utterance("user", "Нет комнаты", Instant::now()).is_err());
        session.create().unwrap();
        let first = session.room.as_ref().unwrap().id.clone();
        for i in 0..40 { session.add_utterance("user", &format!("Вопрос {i}"), Instant::now()).unwrap(); }
        session.add_answer(AnswerState { text: "Полный ответ".repeat(1000), ..Default::default() }).unwrap();
        session.set_summary("Итог первой комнаты".into(), 41).unwrap();
        session.create().unwrap();
        session.add_utterance("user", "Другая комната", Instant::now()).unwrap();
        session.open(&first).unwrap();
        assert_eq!(session.conversation.entries().len(), 41);
        assert_eq!(session.room.as_ref().unwrap().answer.text, "Полный ответ".repeat(1000));
        assert!(!session.conversation.entries().iter().any(|e| e.text == "Другая комната"));
        drop(session);
        let restored = RoomStore::open(path.clone()).unwrap().load(&first).unwrap();
        assert_eq!(restored.summary_through, 41);
        assert_eq!(restored.entries.len(), 41);
        cleanup(path);
    }
    #[test]
    fn recovery_preserves_unsaved_active_memory_before_activating_backup() {
        let path=std::env::temp_dir().join(format!("sobesednik-session-{:032x}",rand::random::<u128>()));
        let mut session=RoomSession::new(RoomStore::open(path.clone()).unwrap());session.create().unwrap();
        let id=session.room.as_ref().unwrap().id.clone();
        session.add_utterance("user","Первая реплика",Instant::now()).unwrap();
        session.add_utterance("user","Вторая реплика",Instant::now()).unwrap();
        let primary=path.join(format!("{id}.json"));std::fs::write(&primary,b"broken primary").unwrap();
        assert!(session.add_utterance("user","Третья несохранённая реплика",Instant::now()).is_err());
        let result=session.recover(&id).unwrap();
        let saved:Room=serde_json::from_slice(&std::fs::read(result.preserved_memory_path.unwrap()).unwrap()).unwrap();
        assert_eq!(saved.entries.len(),3);assert!(saved.entries[2].text.contains("Третья"));
        assert_eq!(std::fs::read(result.preserved_path.unwrap()).unwrap(),b"broken primary");
        assert_eq!(session.conversation.entries().len(),1);assert_eq!(session.room.as_ref().unwrap().id,id);
        assert_eq!(session.store.load(&id).unwrap().entries.len(),1);
        drop(session);cleanup(path);
    }
    #[test]
    fn recovery_of_another_room_saves_current_history_and_preserves_it_on_failure() {
        let path=std::env::temp_dir().join(format!("sobesednik-session-{:032x}",rand::random::<u128>()));
        let mut session=RoomSession::new(RoomStore::open(path.clone()).unwrap());session.create().unwrap();
        let first=session.room.as_ref().unwrap().id.clone();session.add_utterance("user","Первая комната",Instant::now()).unwrap();
        session.create().unwrap();let second=session.room.as_ref().unwrap().id.clone();
        session.add_utterance("user","Вторая комната",Instant::now()).unwrap();
        let first_path=path.join(format!("{first}.json"));std::fs::write(&first_path,b"broken first").unwrap();
        let second_path=path.join(format!("{second}.json"));let second_bytes=std::fs::read(&second_path).unwrap();
        std::fs::write(&second_path,b"broken second").unwrap();
        assert!(session.recover(&first).is_err());assert_eq!(session.room.as_ref().unwrap().id,second);
        assert_eq!(session.conversation.entries()[0].text,"Вторая комната");
        assert_eq!(std::fs::read(&first_path).unwrap(),b"broken first");
        std::fs::write(&second_path,second_bytes).unwrap();
        let result=session.recover(&first).unwrap();assert!(result.preserved_memory_path.is_none());
        assert_eq!(session.room.as_ref().unwrap().id,first);assert_eq!(session.conversation.entries()[0].text,"Первая комната");
        assert_eq!(session.store.load(&second).unwrap().entries[0].text,"Вторая комната");
        drop(session);cleanup(path);
    }
    #[test]
    fn failed_memory_preservation_does_not_restore_or_discard_active_history() {
        let path=std::env::temp_dir().join(format!("sobesednik-session-{:032x}",rand::random::<u128>()));
        let mut session=RoomSession::new(RoomStore::open(path.clone()).unwrap());session.create().unwrap();
        let id=session.room.as_ref().unwrap().id.clone();session.add_utterance("user","Несохранённый контекст",Instant::now()).unwrap();
        let primary=path.join(format!("{id}.json"));std::fs::write(&primary,b"broken primary").unwrap();
        session.room.as_mut().unwrap().summary_through=100;
        assert!(session.recover(&id).unwrap_err().contains("Итоги или память"));assert_eq!(session.conversation.entries().len(),1);
        assert_eq!(std::fs::read(primary).unwrap(),b"broken primary");
        drop(session);cleanup(path);
    }
}
