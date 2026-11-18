use crate::core::{AnswerState, TranscriptEntry};
use serde::{Deserialize, Serialize};
use std::{fs::{self, File, OpenOptions}, io::{ErrorKind, Write}, path::{Path, PathBuf}, sync::Mutex};
use rand::{rngs::OsRng, RngCore};
use sha2::{Digest, Sha256};
#[cfg(windows)]
use std::os::windows::fs::OpenOptionsExt;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Room {
    pub id: String,
    pub title: String,
    pub title_manual: bool,
    pub pinned: bool,
    pub created_at: String,
    pub updated_at: String,
    pub entries: Vec<TranscriptEntry>,
    pub answer: AnswerState,
    pub summary: String,
    pub summary_through: usize,
    pub memory: String,
    pub memory_through: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomInfo {
    pub id: String, pub title: String, pub pinned: bool,
    pub created_at: String, pub updated_at: String, pub entry_count: usize,
    pub summary: String, pub summary_stale: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomRecoveryInfo {
    pub id:String, pub primary_error:Option<String>, pub backup:Option<RoomInfo>,
    pub backup_error:Option<String>, pub available:bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomRecoveryResult {
    pub room:Room, pub preserved_path:Option<String>, pub preserved_memory_path:Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomSearchHit { pub room:RoomInfo, pub snippet:String, pub entry_id:Option<String> }

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RoomTrashInfo {
    pub id:String, pub title:String, pub deleted_at:String, pub entry_count:Option<usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ArtifactStamp { size:u64, sha256:String }
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TrashMarker { version:u8, info:RoomTrashInfo, primary:Option<ArtifactStamp>, backup:Option<ArtifactStamp> }

impl Room {
    pub fn info(&self) -> RoomInfo { RoomInfo { id:self.id.clone(),title:self.title.clone(),pinned:self.pinned,
        created_at:self.created_at.clone(),updated_at:self.updated_at.clone(),entry_count:self.entries.len(),
        summary:self.summary.clone(),summary_stale:!self.summary.trim().is_empty() && self.summary_through < self.entries.len() } }
    pub fn refresh_title(&mut self) {
        if !self.title_manual {
            self.title = self.entries.iter().filter(|entry| entry.source != "assistant")
                .filter_map(|entry| topic_title(&entry.text)).next().unwrap_or_else(|| "Новая комната".into());
        }
        self.updated_at = now();
    }
    pub fn rename(&mut self, title: &str) -> Result<(),String> {
        if title.chars().any(char::is_control) { return Err("Название не должно содержать управляющие символы".into()); }
        let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
        if title.is_empty() || title.chars().count() > 120 { return Err("Название должно содержать от 1 до 120 символов".into()); }
        self.title = title; self.title_manual = true; self.updated_at = now(); Ok(())
    }
    pub fn markdown(&self) -> String {
        let mut result = format!("# {}\n\nСоздана: {}  \nОбновлена: {}\n", self.title, self.created_at, self.updated_at);
        if !self.summary.trim().is_empty() {
            result.push_str("\n## Краткие итоги\n\n");
            result.push_str(&self.summary);
            if self.summary_through < self.entries.len() { result.push_str("\n\n_После составления итогов появились новые реплики._"); }
            result.push('\n');
        }
        result.push_str("\n## Полная история\n");
        if self.entries.is_empty() { result.push_str("\n_История пока пуста._\n"); }
        for entry in &self.entries {
            let label = match entry.source.as_str() { "microphone" => "Микрофон", "system" => "Системный звук", "assistant" => "Ответ", _ => "Реплика" };
            result.push_str(&format!("\n### {label} · {}\n\n{}\n", entry.at, entry.text));
        }
        if !self.answer.text.trim().is_empty() && !self.entries.iter().any(|entry|entry.source=="assistant" && entry.text==self.answer.text) {
            result.push_str(if self.answer.streaming { "\n## Незавершённый ответ\n\n" } else { "\n## Последний ответ\n\n" });
            result.push_str(&self.answer.text); result.push('\n');
        }
        result
    }
}

fn now() -> String { chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true) }

fn random_id() -> Result<String,String> {
    let mut bytes = [0u8;16];
    OsRng.try_fill_bytes(&mut bytes).map_err(|_| "Не удалось создать безопасный идентификатор комнаты")?;
    Ok(bytes.iter().map(|value|format!("{value:02x}")).collect())
}

pub(crate) fn valid_id(id:&str) -> bool { id.len()==32 && id.bytes().all(|b|b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) }

fn topic_title(text:&str) -> Option<String> {
    let text = text.trim().strip_prefix("[Возможен пропуск речи]").unwrap_or(text.trim());
    let compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let normalized: String = compact.to_lowercase().chars().map(|ch|if ch.is_alphanumeric(){ch}else{' '}).collect();
    let words: Vec<_> = normalized.split_whitespace().collect();
    if words.is_empty() { return None; }
    let greeting = words.join(" ");
    if ["привет", "здравствуйте", "добрый день", "доброе утро", "добрый вечер", "привет как дела", "здравствуйте как дела", "как меня слышно", "меня слышно", "hello", "hi"].contains(&greeting.as_str()) {
        return None;
    }
    if words.iter().all(|word|["алло","проверка","тест","тестирование","микрофон","микрофона","связи","раз","два","три"].contains(word)) { return None; }
    if words.len() < 3 && compact.chars().count() < 24 { return None; }
    let mut title: String = compact.chars().take(60).collect();
    if compact.chars().count() > 60 { title = compact.chars().take(59).collect(); title.push('…'); }
    Some(title)
}

fn validate_room(room:&Room) -> Result<(),String> {
    if !valid_id(&room.id) { return Err("Некорректный идентификатор комнаты".into()); }
    if room.title.trim().is_empty() || room.title.chars().count()>120 || room.title.chars().any(char::is_control) {
        return Err("Некорректное название комнаты".into());
    }
    if chrono::DateTime::parse_from_rfc3339(&room.created_at).is_err() || chrono::DateTime::parse_from_rfc3339(&room.updated_at).is_err() {
        return Err("Некорректные даты комнаты".into());
    }
    if room.summary_through > room.entries.len() || room.memory_through > room.entries.len() {
        return Err("Итоги или память ссылаются на отсутствующую историю".into());
    }
    if (room.summary_through > 0 && room.summary.trim().is_empty()) || (room.memory_through > 0 && room.memory.trim().is_empty()) {
        return Err("Для отмеченной истории отсутствуют итоги или память".into());
    }
    for entry in &room.entries {
        if !["microphone", "system", "user", "assistant"].contains(&entry.source.as_str()) {
            return Err("В истории комнаты указан неизвестный источник реплики".into());
        }
        if chrono::DateTime::parse_from_rfc3339(&entry.at).is_err() {
            return Err("В истории комнаты указана некорректная дата реплики".into());
        }
    }
    Ok(())
}

#[derive(Debug)]
pub struct RoomStore { root:PathBuf, access:Mutex<()>, _process_lock:File }
impl RoomStore {
    fn trash_directory(&self,create:bool)->Result<Option<PathBuf>,String> {
        let path=self.root.join("trash");
        if create {
            match fs::create_dir(&path) { Ok(())=>{},Err(error) if error.kind()==ErrorKind::AlreadyExists=>{},Err(_)=>return Err(format!("Не удалось создать корзину: {}",path.display())) }
        }
        let metadata=match fs::symlink_metadata(&path) {
            Ok(value)=>value,Err(error) if error.kind()==ErrorKind::NotFound=>return Ok(None),Err(_)=>return Err(format!("Не удалось проверить корзину: {}",path.display())),
        };
        if !metadata.is_dir()||is_link(&metadata) { return Err(format!("Корзина должна быть обычной папкой: {}",path.display())); }
        let resolved=path.canonicalize().map_err(|_|format!("Не удалось открыть корзину: {}",path.display()))?;
        if resolved.parent()!=Some(self.root.as_path()) { return Err(format!("Корзина находится за пределами хранилища: {}",path.display())); }
        Ok(Some(path))
    }
    fn marker_path(&self,id:&str,create:bool)->Result<PathBuf,String> {
        if !valid_id(id) { return Err("Некорректный идентификатор комнаты".into()); }
        let directory=self.trash_directory(create)?.unwrap_or_else(||self.root.join("trash"));
        Ok(directory.join(format!("{id}.json")))
    }
    fn read_marker(&self,id:&str)->Result<TrashMarker,String> {
        let path=self.marker_path(id,false)?;
        let metadata=fs::symlink_metadata(&path).map_err(|_|format!("Комната не найдена в корзине: {}",path.display()))?;
        if !metadata.is_file()||is_link(&metadata)||metadata.len()>65536 { return Err(format!("Некорректный файл корзины: {}",path.display())); }
        let bytes=fs::read(&path).map_err(|_|format!("Не удалось прочитать корзину: {}",path.display()))?;
        let marker:TrashMarker=serde_json::from_slice(&bytes).map_err(|_|format!("Повреждена запись корзины; исходная история сохранена: {}",path.display()))?;
        let valid_stamp=|stamp:&ArtifactStamp|stamp.sha256.len()==64&&stamp.sha256.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b));
        if marker.version!=1||marker.info.id!=id||marker.info.title.trim().is_empty()||marker.info.title.chars().count()>120
            ||marker.info.title.chars().any(char::is_control)||chrono::DateTime::parse_from_rfc3339(&marker.info.deleted_at).is_err()
            ||(marker.primary.is_none()&&marker.backup.is_none())||marker.primary.iter().chain(marker.backup.iter()).any(|stamp|!valid_stamp(stamp)) {
            return Err(format!("Некорректные сведения корзины; исходная история сохранена: {}",path.display()));
        }
        Ok(marker)
    }
    fn is_trashed(&self,id:&str)->Result<bool,String> { file_exists(&self.marker_path(id,false)?) }
    fn ensure_active(&self,id:&str)->Result<(),String> {
        if self.is_trashed(id)? { self.read_marker(id)?;return Err("Комната находится в корзине. Сначала восстанови её".into()); }
        Ok(())
    }
    fn hidden_rooms(&self)->Result<(std::collections::BTreeSet<String>,Vec<RoomRecoveryInfo>),String> {
        let mut hidden=std::collections::BTreeSet::new();let mut issues=Vec::new();
        let Some(directory)=self.trash_directory(false)? else { return Ok((hidden,issues)); };
        for entry in fs::read_dir(&directory).map_err(|_|format!("Не удалось прочитать корзину: {}",directory.display()))? {
            let entry=entry.map_err(|_|"Не удалось прочитать запись корзины")?;let path=entry.path();
            if path.extension().and_then(|extension|extension.to_str())!=Some("json") { continue; }
            let id=path.file_stem().and_then(|value|value.to_str()).unwrap_or_default();
            let error=if valid_id(id) { hidden.insert(id.to_owned());self.read_marker(id).err() }
                else { Some(format!("Некорректное имя записи корзины: {}",path.display())) };
            if let Some(error)=error { issues.push(RoomRecoveryInfo {id:if valid_id(id){id.into()}else{format!("trash/{}",entry.file_name().to_string_lossy())},primary_error:Some(error),backup:None,backup_error:None,available:false}); }
        }
        Ok((hidden,issues))
    }
    pub fn trash_list(&self)->Result<Vec<RoomTrashInfo>,String> {
        let _guard=self.access.lock().map_err(|_|"Хранилище комнат требует перезапуска после ошибки")?;
        let (ids,issues)=self.hidden_rooms()?;
        if let Some(issue)=issues.first() { return Err(issue.primary_error.clone().unwrap_or_else(||"Повреждена запись корзины".into())); }
        let mut entries=ids.iter().map(|id|self.read_marker(id).map(|marker|marker.info)).collect::<Result<Vec<_>,_>>()?;
        entries.sort_by(|a,b|chrono::DateTime::parse_from_rfc3339(&b.deleted_at).ok().cmp(&chrono::DateTime::parse_from_rfc3339(&a.deleted_at).ok()).then_with(||a.id.cmp(&b.id)));
        Ok(entries)
    }
    pub fn trash_room(&self,id:&str)->Result<RoomTrashInfo,String> {
        let _guard=self.access.lock().map_err(|_|"Хранилище комнат требует перезапуска после ошибки")?;
        self.ensure_active(id)?;
        let path=self.path(id)?;let primary=snapshot_file(&path)?;let backup=snapshot_file(&path.with_extension("bak"))?;
        if primary.is_none()&&backup.is_none() { return Err("Комната не найдена".into()); }
        let decode=|snapshot:&Option<(ArtifactStamp,Vec<u8>)>|snapshot.as_ref().and_then(|(_,bytes)|serde_json::from_slice::<Room>(bytes).ok())
            .filter(|room|room.id==id&&validate_room(room).is_ok());
        let valid_primary=decode(&primary);let valid_backup=decode(&backup);
        let title=valid_primary.as_ref().or(valid_backup.as_ref()).map(|room|room.title.clone()).unwrap_or_else(||format!("Комната {}",&id[..8]));
        let info=RoomTrashInfo{id:id.into(),title,deleted_at:now(),entry_count:valid_primary.as_ref().map(|room|room.entries.len())};
        let marker=TrashMarker{version:1,info:info.clone(),primary:primary.map(|(stamp,_)|stamp),backup:backup.map(|(stamp,_)|stamp)};
        let bytes=serde_json::to_vec_pretty(&marker).map_err(|_|"Не удалось подготовить запись корзины")?;
        atomic_create(&self.marker_path(id,true)?,&bytes)?;
        Ok(info)
    }
    pub fn restore_room(&self,id:&str)->Result<(),String> {
        let _guard=self.access.lock().map_err(|_|"Хранилище комнат требует перезапуска после ошибки")?;
        let marker=self.read_marker(id)?;let path=self.path(id)?;
        if snapshot_file(&path)?.map(|(stamp,_)|stamp)!=marker.primary||snapshot_file(&path.with_extension("bak"))?.map(|(stamp,_)|stamp)!=marker.backup {
            return Err(format!("Файлы удалённой комнаты изменились. Корзина и исходные файлы не изменены: {}",path.display()));
        }
        let marker_path=self.marker_path(id,false)?;
        fs::remove_file(&marker_path).map_err(|_|format!("Не удалось восстановить комнату из корзины: {}",marker_path.display()))
    }
    pub fn open(root:PathBuf) -> Result<Self,String> {
        if root.as_os_str().is_empty() { return Err("Не задана папка комнат".into()); }
        fs::create_dir_all(&root).map_err(|_| "Не удалось создать папку комнат")?;
        let root = root.canonicalize().map_err(|_| "Не удалось открыть папку комнат")?;
        let lock_path = root.join(".lock");
        if fs::symlink_metadata(&lock_path).is_ok_and(|meta| meta.file_type().is_symlink()) {
            return Err("Файл блокировки хранилища не должен быть ссылкой".into());
        }
        let mut options=OpenOptions::new(); options.create(true).read(true).write(true);
        #[cfg(windows)]
        options.share_mode(0);
        let process_lock=options.open(lock_path).map_err(|_| "Хранилище комнат уже открыто другим экземпляром приложения или недоступно для записи")?;
        #[cfg(not(windows))]
        process_lock.try_lock().map_err(|_| "Хранилище комнат уже открыто другим экземпляром приложения")?;
        Ok(Self { root, access:Mutex::new(()), _process_lock:process_lock })
    }
    fn path(&self,id:&str)->Result<PathBuf,String> {
        if !valid_id(id) { return Err("Некорректный идентификатор комнаты".into()); }
        Ok(self.root.join(format!("{id}.json")))
    }
    fn read_room(&self,id:&str)->Result<Room,String> {
        self.ensure_active(id)?;
        self.read_file(id,&self.path(id)?).map(|(room,_)|room)
    }
    fn read_file(&self,id:&str,path:&Path)->Result<(Room,Vec<u8>),String> {
        let location=path.display().to_string();
        let metadata=fs::symlink_metadata(&path).map_err(|_| format!("Не удалось открыть комнату: {location}"))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() { return Err(format!("Комната не является обычным файлом: {location}")); }
        let bytes=fs::read(&path).map_err(|_| format!("Не удалось прочитать комнату: {location}"))?;
        let room:Room=serde_json::from_slice(&bytes).map_err(|_| format!("Комната повреждена или имеет неподдерживаемый формат. Файл не изменён: {location}"))?;
        if room.id!=id { return Err(format!("Идентификатор комнаты не совпадает с именем файла. Файл не изменён: {location}")); }
        validate_room(&room).map_err(|error|format!("{error}. Файл не изменён: {location}"))?;
        Ok((room,bytes))
    }
    pub fn list(&self) -> Result<Vec<RoomInfo>,String> {
        let _guard=self.access.lock().map_err(|_| "Хранилище комнат требует перезапуска после ошибки")?;
        let mut rooms=self.room_ids(false)?.iter().map(|id|self.read_room(id).map(|room|room.info())).collect::<Result<Vec<_>,_>>()?;
        rooms.sort_by(compare_rooms);
        Ok(rooms)
    }
    pub fn list_available(&self)->Result<(Vec<RoomInfo>,Vec<RoomRecoveryInfo>),String> {
        let (ids,mut issues)=self.scan_ids(true)?;let mut rooms=Vec::new();
        for id in ids {
            let primary_error=match self.read_room(&id) { Ok(room)=>{ rooms.push(room.info());None },Err(error)=>Some(error) };
            let info=self.recovery_state_with_error(&id,primary_error)?;
            if info.primary_error.is_some()||info.backup_error.is_some() { issues.push(info); }
        }
        let (hidden,marker_issues)=self.hidden_rooms()?;
        rooms.retain(|room|!hidden.contains(&room.id));issues.retain(|issue|!hidden.contains(&issue.id));issues.extend(marker_issues);
        rooms.sort_by(compare_rooms);issues.sort_by(|a,b|a.id.cmp(&b.id));issues.dedup_by(|a,b|a.id==b.id);
        Ok((rooms,issues))
    }
    fn room_ids(&self,include_backups:bool)->Result<Vec<String>,String> {
        let (ids,issues)=self.scan_ids(include_backups)?;
        if let Some(issue)=issues.into_iter().next() { return Err(issue.primary_error.or(issue.backup_error).unwrap_or_else(||"Некорректное имя файла комнаты".into())); }
        Ok(ids)
    }
    fn scan_ids(&self,include_backups:bool)->Result<(Vec<String>,Vec<RoomRecoveryInfo>),String> {
        let (hidden,mut issues)=self.hidden_rooms()?;let mut ids=std::collections::BTreeSet::new();
        for entry in fs::read_dir(&self.root).map_err(|_| "Не удалось прочитать список комнат")? {
            let entry=entry.map_err(|_| "Не удалось прочитать файл комнаты")?;
            let path=entry.path();
            match path.extension().and_then(|value|value.to_str()) { Some("json")=>{},Some("bak") if include_backups=>{},_=>continue }
            let id=path.file_stem().and_then(|name|name.to_str()).unwrap_or_default();
            if valid_id(id) { if !hidden.contains(id) { ids.insert(id.to_owned()); } continue; }
            let error=Some(format!("Некорректное имя файла комнаты: {}",path.display()));
            let is_backup=path.extension().is_some_and(|extension|extension=="bak");
            issues.push(RoomRecoveryInfo { id:entry.file_name().to_string_lossy().into_owned(),primary_error:if is_backup {None}else{error.clone()},
                backup:None,backup_error:if is_backup {error}else{None},available:false });
        }
        Ok((ids.into_iter().collect(),issues))
    }
    pub fn create(&self) -> Result<Room,String> {
        let _guard=self.access.lock().map_err(|_| "Хранилище комнат требует перезапуска после ошибки")?;
        for _ in 0..10 {
            let id=random_id()?;
            let path=self.path(&id)?;
            if file_exists(&path)? || file_exists(&path.with_extension("bak"))? || self.is_trashed(&id)? { continue; }
            let created=now();
            let room=Room { id,title:"Новая комната".into(),title_manual:false,pinned:false,
                created_at:created.clone(),updated_at:created,entries:vec![],answer:AnswerState::default(),
                summary:String::new(),summary_through:0,memory:String::new(),memory_through:0 };
            self.write_room(&room)?; return Ok(room);
        }
        Err("Не удалось подобрать уникальный идентификатор комнаты".into())
    }
    pub fn load(&self,id:&str) -> Result<Room,String> {
        let _guard=self.access.lock().map_err(|_| "Хранилище комнат требует перезапуска после ошибки")?;
        self.read_room(id)
    }
    pub fn save(&self,room:&Room) -> Result<(),String> {
        let _guard=self.access.lock().map_err(|_| "Хранилище комнат требует перезапуска после ошибки")?;
        self.write_room(room)
    }
    /// Returns matches in healthy rooms; callers must display file issues from list_available().
    pub fn search(&self,query:&str)->Result<Vec<RoomSearchHit>,String> {
        if query.chars().count()>200 { return Err("Поисковый запрос должен содержать не больше 200 символов".into()); }
        if query.chars().any(|ch|ch.is_control()&&!ch.is_whitespace()) { return Err("Поисковый запрос содержит управляющие символы".into()); }
        let words:Vec<_>=query.split_whitespace().map(str::to_lowercase).collect();
        // Each JSON is atomically replaced, so a reader gets a complete version without blocking saves.
        let mut hits=Vec::new();
        for id in self.scan_ids(false)?.0 {
            if let Ok(room)=self.read_room(&id) {
                if let Some(hit)=search_room(&room,&words) { hits.push(hit); }
            }
        }
        let hidden=self.hidden_rooms()?.0;hits.retain(|hit|!hidden.contains(&hit.room.id));
        hits.sort_by(|a,b|compare_rooms(&a.room,&b.room));
        Ok(hits)
    }
    pub fn recovery_info(&self,id:&str)->Result<RoomRecoveryInfo,String> {
        let _guard=self.access.lock().map_err(|_| "Хранилище комнат требует перезапуска после ошибки")?;
        self.recovery_state(id)
    }
    fn recovery_state(&self,id:&str)->Result<RoomRecoveryInfo,String> {
        self.ensure_active(id)?;
        let primary_error=self.read_file(id,&self.path(id)?).err();
        self.recovery_state_with_error(id,primary_error)
    }
    fn recovery_state_with_error(&self,id:&str,primary_error:Option<String>)->Result<RoomRecoveryInfo,String> {
        let path=self.path(id)?;
        let backup_path=path.with_extension("bak");
        let (backup,backup_error)=match file_exists(&backup_path) {
            Ok(true)=>match self.read_file(id,&backup_path) { Ok((room,_))=>(Some(room.info()),None),Err(error)=>(None,Some(error)) },
            Ok(false)=>(None,None),Err(error)=>(None,Some(error)),
        };
        let available=primary_error.is_some()&&backup.is_some();
        Ok(RoomRecoveryInfo { id:id.into(),primary_error,backup,backup_error,available })
    }
    pub fn list_recoveries(&self)->Result<Vec<RoomRecoveryInfo>,String> {
        let _guard=self.access.lock().map_err(|_| "Хранилище комнат требует перезапуска после ошибки")?;
        let states=self.room_ids(true)?.iter().map(|id|self.recovery_state(id)).collect::<Result<Vec<_>,_>>()?;
        Ok(states.into_iter().filter(|info|info.primary_error.is_some()||info.backup_error.is_some()).collect())
    }
    pub fn recover(&self,id:&str)->Result<RoomRecoveryResult,String> {
        let _guard=self.access.lock().map_err(|_| "Хранилище комнат требует перезапуска после ошибки")?;
        self.ensure_active(id)?;
        let path=self.path(id)?;
        if self.read_file(id,&path).is_ok() { return Err("Основной файл комнаты исправен; восстановление не требуется".into()); }
        let (room,bytes)=self.read_file(id,&path.with_extension("bak"))?;
        let preserved_path=if file_exists(&path)? {
            let metadata=fs::symlink_metadata(&path).map_err(|_|format!("Не удалось проверить исходный файл: {}",path.display()))?;
            if metadata.file_type().is_symlink()||!metadata.is_file() { return Err(format!("Исходный файл не является обычным файлом: {}",path.display())); }
            let original=fs::read(&path).map_err(|_|format!("Не удалось сохранить повреждённый оригинал: {}",path.display()))?;
            Some(self.preserve_bytes(id,"corrupt",&original)?)
        } else { None };
        atomic_write(&path,&bytes).map_err(|error|match &preserved_path { Some(saved)=>format!("{error}. Исходный файл сохранён: {saved}"),None=>error })?;
        Ok(RoomRecoveryResult { room,preserved_path,preserved_memory_path:None })
    }
    pub(crate) fn preserve_unsaved(&self,room:&Room)->Result<String,String> {
        let _guard=self.access.lock().map_err(|_| "Хранилище комнат требует перезапуска после ошибки")?;
        validate_room(room)?;
        self.ensure_active(&room.id)?;
        let bytes=serde_json::to_vec_pretty(room).map_err(|_|"Не удалось подготовить несохранённую историю")?;
        self.preserve_bytes(&room.id,"unsaved",&bytes)
    }
    fn preserve_bytes(&self,id:&str,extension:&str,bytes:&[u8])->Result<String,String> {
        for _ in 0..10 {
            let path=self.root.join(format!("{id}.{}.{extension}",random_id()?));
            if file_exists(&path)? { continue; }
            atomic_write(&path,bytes)?;
            return Ok(path.display().to_string());
        }
        Err("Не удалось подобрать имя для сохранения исходной истории".into())
    }
    fn write_room(&self,room:&Room)->Result<(),String> {
        validate_room(room)?;
        self.ensure_active(&room.id)?;
        let path=self.path(&room.id)?;
        let previous=if file_exists(&path)? { Some(self.read_file(&room.id,&path)?.1) } else { None };
        let backup=path.with_extension("bak");
        if file_exists(&backup)? {
            self.read_file(&room.id,&backup)?;
            if previous.is_none() { return Err(format!("Основной файл комнаты отсутствует. Сначала восстанови резервную копию: {}",backup.display())); }
        }
        let bytes=serde_json::to_vec_pretty(room).map_err(|_| "Не удалось подготовить комнату к сохранению")?;
        if let Some(previous)=previous { atomic_write(&backup,&previous)?; }
        atomic_write(&path,&bytes)
    }
}

fn compare_rooms(a:&RoomInfo,b:&RoomInfo)->std::cmp::Ordering {
    b.pinned.cmp(&a.pinned).then_with(||chrono::DateTime::parse_from_rfc3339(&b.updated_at).ok()
        .cmp(&chrono::DateTime::parse_from_rfc3339(&a.updated_at).ok())).then_with(||a.id.cmp(&b.id))
}

fn search_room(room:&Room,words:&[String])->Option<RoomSearchHit> {
    let title=room.title.to_lowercase();let summary=room.summary.to_lowercase();
    let mut found:Vec<_>=words.iter().map(|word|title.contains(word)||summary.contains(word)).collect();
    let mut best_entry=None;let mut best_count=0;
    for entry in &room.entries {
        let text=entry.text.to_lowercase();let mut count=0;
        for (index,word) in words.iter().enumerate() {
            if text.contains(word) { found[index]=true;count+=1; }
        }
        if count>best_count { best_count=count;best_entry=Some(entry); }
    }
    if found.iter().any(|matched|!matched) { return None; }
    if words.is_empty() { best_entry=room.entries.last(); }
    let text=if let Some(entry)=best_entry { &entry.text }
        else if !room.summary.is_empty()&&(words.is_empty()||words.iter().any(|word|summary.contains(word))) { &room.summary }
        else { &room.title };
    Some(RoomSearchHit { room:room.info(),snippet:excerpt(text,words),entry_id:best_entry.map(|entry|entry.id.clone()) })
}

fn excerpt(text:&str,words:&[String])->String {
    let compact=text.split_whitespace().collect::<Vec<_>>().join(" ");
    let chars:Vec<_>=compact.chars().collect();
    if chars.len()<=220 { return compact; }
    let lower=compact.to_lowercase();
    let offset=words.iter().filter_map(|word|lower.find(word)).min().unwrap_or(0);
    let mut folded_bytes=0;
    let anchor=chars.iter().position(|ch| {
        let at_match=folded_bytes>=offset;folded_bytes+=ch.to_lowercase().map(char::len_utf8).sum::<usize>();at_match
    }).unwrap_or(0);
    let before=30.min(218usize.saturating_sub(words.iter().map(|word|word.chars().count()).max().unwrap_or(0)));
    let start=anchor.saturating_sub(before).min(chars.len().saturating_sub(218));
    let end=(start+218).min(chars.len());
    let mut snippet=String::new();if start>0 { snippet.push('…'); }
    snippet.extend(&chars[start..end]);if end<chars.len() { snippet.push('…'); }snippet
}

fn file_exists(path:&Path)->Result<bool,String> {
    match fs::symlink_metadata(path) {
        Ok(_)=>Ok(true), Err(error) if error.kind()==ErrorKind::NotFound=>Ok(false),
        Err(_)=>Err(format!("Не удалось проверить файл: {}",path.display())),
    }
}

fn is_link(metadata:&fs::Metadata)->bool {
    #[cfg(windows)]
    { use std::os::windows::fs::MetadataExt;if metadata.file_attributes()&0x400!=0 { return true; } }
    metadata.file_type().is_symlink()
}

fn snapshot_file(path:&Path)->Result<Option<(ArtifactStamp,Vec<u8>)>,String> {
    let metadata=match fs::symlink_metadata(path) { Ok(value)=>value,Err(error) if error.kind()==ErrorKind::NotFound=>return Ok(None),Err(_)=>return Err(format!("Не удалось проверить файл комнаты: {}",path.display())) };
    if !metadata.is_file()||is_link(&metadata) { return Err(format!("Файл комнаты должен быть обычным файлом: {}",path.display())); }
    let bytes=fs::read(path).map_err(|_|format!("Не удалось прочитать файл комнаты: {}",path.display()))?;
    let stamp=ArtifactStamp{size:bytes.len() as u64,sha256:format!("{:x}",Sha256::digest(&bytes))};
    Ok(Some((stamp,bytes)))
}

fn atomic_create(path:&Path,bytes:&[u8])->Result<(),String> {
    let temporary=path.with_extension(format!("{}.tmp",random_id()?));
    let result=(||->Result<(),String> {
        let mut file=OpenOptions::new().write(true).create_new(true).open(&temporary).map_err(|_|format!("Не удалось создать запись корзины: {}",temporary.display()))?;
        file.write_all(bytes).and_then(|_|file.sync_all()).map_err(|_|format!("Не удалось записать корзину: {}",path.display()))?;drop(file);
        // Publishing a completed hard link is atomic and cannot overwrite an existing marker.
        fs::hard_link(&temporary,path).map_err(|_|format!("Не удалось поместить комнату в корзину; запись уже существует или недоступна: {}",path.display()))
    })();
    let _=fs::remove_file(&temporary);result
}

fn atomic_write(path:&Path,bytes:&[u8])->Result<(),String> {
    let temporary=path.with_extension(format!("{}.tmp",random_id()?));
    let write=(||->Result<(),String> {
        let mut file=OpenOptions::new().write(true).create_new(true).open(&temporary)
            .map_err(|_|format!("Не удалось создать временный файл: {}",temporary.display()))?;
        file.write_all(bytes).and_then(|_|file.sync_all())
            .map_err(|_|format!("Не удалось завершить запись на диск: {}",path.display()))?;
        drop(file);
        fs::rename(&temporary,path).map_err(|_|format!("Не удалось атомарно заменить файл: {}",path.display()))
    })();
    if write.is_err() { let _=fs::remove_file(&temporary); }
    write
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    struct TestDirectory(PathBuf);
    impl TestDirectory {
        fn new() -> Self {
            let path=std::env::temp_dir().join(format!("sobesednik-room-tests-{}-{}",std::process::id(),rand::random::<u64>()));
            fs::create_dir(&path).unwrap(); Self(path)
        }
    }
    impl Drop for TestDirectory {
        fn drop(&mut self) {
            if let Ok(entries)=fs::read_dir(&self.0) {
                for entry in entries.flatten() {
                    if entry.file_name()=="trash"&&entry.file_type().is_ok_and(|kind|kind.is_dir()) {
                        if let Ok(files)=fs::read_dir(entry.path()) { for file in files.flatten() { let _=fs::remove_file(file.path()); } }
                        let _=fs::remove_dir(entry.path());
                    } else { let _=fs::remove_file(entry.path()); }
                }
            }
            let _=fs::remove_dir(&self.0);
        }
    }
    fn entry(id:usize,text:&str)->TranscriptEntry { TranscriptEntry{id:id.to_string(),source:"microphone".into(),text:text.into(),at:"2026-11-16T00:00:00Z".into()} }
    #[test]
    fn trash_survives_restart_and_preserves_full_primary_and_backup() {
        let dir=TestDirectory::new();let id;let primary;let backup;
        {
            let store=RoomStore::open(dir.0.clone()).unwrap();let mut room=store.create().unwrap();id=room.id.clone();
            room.rename("Полная история в корзине").unwrap();
            room.entries=(0..240).map(|i|entry(i,&format!("Сохранённая реплика {i}"))).collect();store.save(&room).unwrap();
            room.entries.push(entry(240,"Последняя реплика"));store.save(&room).unwrap();
            primary=fs::read(dir.0.join(format!("{id}.json"))).unwrap();backup=fs::read(dir.0.join(format!("{id}.bak"))).unwrap();
            let trashed=store.trash_room(&id).unwrap();assert_eq!(trashed.entry_count,Some(241));
            assert_eq!(trashed.title,"Полная история в корзине");
            assert!(store.list().unwrap().is_empty());assert!(store.list_available().unwrap().0.is_empty());assert!(store.search("реплика").unwrap().is_empty());
            assert!(store.load(&id).is_err());assert!(store.save(&room).is_err());assert!(store.recover(&id).is_err());assert!(store.recovery_info(&id).is_err());
            assert!(store.list_recoveries().unwrap().is_empty());
            assert_eq!(fs::read(dir.0.join(format!("{id}.json"))).unwrap(),primary);assert_eq!(fs::read(dir.0.join(format!("{id}.bak"))).unwrap(),backup);
            assert!(store.trash_room(&id).is_err());
        }
        let store=RoomStore::open(dir.0.clone()).unwrap();assert_eq!(store.trash_list().unwrap().len(),1);
        store.restore_room(&id).unwrap();assert!(store.trash_list().unwrap().is_empty());assert!(store.restore_room(&id).is_err());
        assert_eq!(store.load(&id).unwrap().entries.len(),241);assert_eq!(store.search("Последняя").unwrap().len(),1);
        assert_eq!(fs::read(dir.0.join(format!("{id}.json"))).unwrap(),primary);assert_eq!(fs::read(dir.0.join(format!("{id}.bak"))).unwrap(),backup);
    }
    #[test]
    fn corrupt_room_can_be_trashed_and_restored_without_erasing_its_bytes() {
        let dir=TestDirectory::new();let store=RoomStore::open(dir.0.clone()).unwrap();let room=store.create().unwrap();
        let primary=dir.0.join(format!("{}.json",room.id));fs::write(&primary,b"corrupt original stays intact").unwrap();
        let trashed=store.trash_room(&room.id).unwrap();assert_eq!(trashed.entry_count,None);
        let (active,issues)=store.list_available().unwrap();assert!(active.is_empty()&&issues.is_empty());
        store.restore_room(&room.id).unwrap();assert!(store.load(&room.id).is_err());
        assert_eq!(store.list_available().unwrap().1.len(),1);assert_eq!(fs::read(primary).unwrap(),b"corrupt original stays intact");
    }
    #[test]
    fn trash_rejects_invalid_ids_and_restore_refuses_changed_data() {
        let dir=TestDirectory::new();let store=RoomStore::open(dir.0.clone()).unwrap();let room=store.create().unwrap();
        for id in ["../escape","..\\escape","C:\\escape","", "AABB"] {
            assert!(store.trash_room(id).is_err());assert!(store.restore_room(id).is_err());
        }
        assert!(store.trash_room(&"a".repeat(32)).is_err());
        store.trash_room(&room.id).unwrap();
        let marker=dir.0.join("trash").join(format!("{}.json",room.id));let marker_before=fs::read(&marker).unwrap();
        let primary=dir.0.join(format!("{}.json",room.id));fs::write(&primary,b"changed outside application").unwrap();
        assert!(store.restore_room(&room.id).is_err());assert_eq!(fs::read(marker).unwrap(),marker_before);
        assert_eq!(fs::read(primary).unwrap(),b"changed outside application");assert!(store.list_available().unwrap().0.is_empty());
    }
    #[test]
    fn malformed_trash_marker_is_reported_without_reactivating_the_room() {
        let dir=TestDirectory::new();let store=RoomStore::open(dir.0.clone()).unwrap();let room=store.create().unwrap();
        store.trash_room(&room.id).unwrap();
        let marker=dir.0.join("trash").join(format!("{}.json",room.id));fs::write(&marker,b"invalid marker").unwrap();
        assert!(store.trash_list().is_err());assert!(store.restore_room(&room.id).is_err());assert!(store.load(&room.id).is_err());
        let (active,issues)=store.list_available().unwrap();assert!(active.is_empty());assert!(issues.iter().any(|issue|issue.primary_error.as_ref().is_some_and(|error|error.contains("trash"))));
        assert_eq!(fs::read(marker).unwrap(),b"invalid marker");
    }
    #[test]
    fn trash_marker_publication_never_overwrites_a_conflicting_file() {
        let dir=TestDirectory::new();let marker=dir.0.join("existing.json");fs::write(&marker,b"existing metadata").unwrap();
        assert!(atomic_create(&marker,b"replacement").is_err());assert_eq!(fs::read(&marker).unwrap(),b"existing metadata");
        assert!(!fs::read_dir(&dir.0).unwrap().any(|entry|entry.unwrap().path().extension().is_some_and(|extension|extension=="tmp")));
    }
    #[test]
    fn invalid_trash_metadata_cannot_unhide_or_modify_room_data() {
        let dir=TestDirectory::new();let store=RoomStore::open(dir.0.clone()).unwrap();let room=store.create().unwrap();
        let path=dir.0.join(format!("{}.json",room.id));let original=fs::read(&path).unwrap();store.trash_room(&room.id).unwrap();
        let marker_path=dir.0.join("trash").join(format!("{}.json",room.id));
        let original_marker:serde_json::Value=serde_json::from_slice(&fs::read(&marker_path).unwrap()).unwrap();
        for kind in 0..5 {
            let mut bad=original_marker.clone();
            match kind {0=>bad["info"]["id"]=serde_json::json!("a".repeat(32)),1=>bad["info"]["deletedAt"]=serde_json::json!("bad date"),
                2=>bad["primary"]["sha256"]=serde_json::json!("not-a-checksum"),3=>bad["info"]["title"]=serde_json::json!("bad\nname"),_=>bad["version"]=serde_json::json!(2)};
            let bytes=serde_json::to_vec(&bad).unwrap();fs::write(&marker_path,&bytes).unwrap();
            assert!(store.restore_room(&room.id).is_err());assert!(store.load(&room.id).is_err());
            assert!(store.list_available().unwrap().0.is_empty());assert_eq!(fs::read(&path).unwrap(),original);assert_eq!(fs::read(&marker_path).unwrap(),bytes);
        }
    }
    #[cfg(windows)]
    #[test]
    fn failed_marker_removal_keeps_room_in_trash_and_all_bytes_intact() {
        let dir=TestDirectory::new();let store=RoomStore::open(dir.0.clone()).unwrap();let room=store.create().unwrap();
        let path=dir.0.join(format!("{}.json",room.id));let original=fs::read(&path).unwrap();store.trash_room(&room.id).unwrap();
        let marker=dir.0.join("trash").join(format!("{}.json",room.id));let marker_bytes=fs::read(&marker).unwrap();
        let locked=OpenOptions::new().read(true).share_mode(3).open(&marker).unwrap();
        assert!(store.restore_room(&room.id).is_err());assert_eq!(fs::read(&marker).unwrap(),marker_bytes);
        assert_eq!(fs::read(&path).unwrap(),original);assert!(store.list_available().unwrap().0.is_empty());
        drop(locked);store.restore_room(&room.id).unwrap();assert!(store.load(&room.id).is_ok());
    }
    #[cfg(windows)]
    #[test]
    #[ignore = "Requires Windows symbolic-link permission; touches only its own temporary directories"]
    fn trash_refuses_symbolic_links_to_external_directories_and_files() {
        use std::os::windows::fs::{symlink_dir,symlink_file};
        let dir=TestDirectory::new();let outside=TestDirectory::new();let store=RoomStore::open(dir.0.clone()).unwrap();let room=store.create().unwrap();
        let trash=dir.0.join("trash");symlink_dir(&outside.0,&trash).unwrap();
        assert!(store.trash_room(&room.id).is_err());assert!(store.trash_list().is_err());assert_eq!(fs::read_dir(&outside.0).unwrap().count(),0);
        fs::remove_dir(&trash).unwrap();fs::create_dir(&trash).unwrap();
        let outside_file=outside.0.join("unchanged.json");fs::write(&outside_file,b"do not touch").unwrap();
        let marker=trash.join(format!("{}.json",room.id));symlink_file(&outside_file,&marker).unwrap();
        assert!(store.restore_room(&room.id).is_err());assert!(store.trash_list().is_err());assert!(store.load(&room.id).is_err());
        assert_eq!(fs::read(&outside_file).unwrap(),b"do not touch");fs::remove_file(marker).unwrap();
    }
    #[test]
    fn auto_title_waits_for_a_topic_and_manual_title_is_preserved() {
        let dir=TestDirectory::new(); let store=RoomStore::open(dir.0.clone()).unwrap(); let mut room=store.create().unwrap();
        room.entries.push(entry(1,"Привет")); room.entries.push(entry(2,"Проверка микрофона, раз два три"));
        room.refresh_title(); assert_eq!(room.title,"Новая комната");
        room.entries.push(entry(3,"Как устроены транзакции PostgreSQL?")); room.refresh_title();
        assert_eq!(room.title,"Как устроены транзакции PostgreSQL?");
        room.rename("  Собеседование   по SQL  ").unwrap();
        room.entries.push(entry(4,"Теперь обсудим совсем другую тему")); room.refresh_title();
        assert_eq!(room.title,"Собеседование по SQL"); assert!(room.title_manual);
    }
    #[test]
    fn titles_are_unicode_safe_and_invalid_manual_names_are_rejected() {
        let dir=TestDirectory::new();let store=RoomStore::open(dir.0.clone()).unwrap();let mut room=store.create().unwrap();
        room.entries.push(entry(1,&"я".repeat(90)));room.refresh_title();
        assert_eq!(room.title.chars().count(),60);assert!(room.title.starts_with('я'));
        assert!(room.rename("   ").is_err());assert!(room.rename("a\0b").is_err());assert!(room.rename(&"я".repeat(121)).is_err());
    }
    #[test]
    fn saves_and_reopens_complete_history_and_generated_materials() {
        let dir=TestDirectory::new();let id;
        {
            let store=RoomStore::open(dir.0.clone()).unwrap();let mut room=store.create().unwrap();id=room.id.clone();
            room.entries=(0..75).map(|i|entry(i,&format!("Полная реплика номер {i}"))).collect();
            room.summary="Итоги разговора".into();room.summary_through=70;room.memory="Рабочая память".into();room.memory_through=60;
            room.answer.text="Последний ответ".into();room.refresh_title();store.save(&room).unwrap();
        }
        let store=RoomStore::open(dir.0.clone()).unwrap();let room=store.load(&id).unwrap();
        assert_eq!(room.entries.len(),75);assert!(room.entries[0].text.ends_with('0'));assert!(room.entries[74].text.ends_with("74"));
        assert_eq!(room.memory,"Рабочая память");assert_eq!(room.summary_through,70);assert_eq!(room.answer.text,"Последний ответ");
        assert!(room.info().summary_stale);
        let markdown=room.markdown();assert!(markdown.contains("номер 0")&&markdown.contains("номер 74")&&markdown.contains("Итоги разговора")&&markdown.contains("Последний ответ"));
    }
    #[test]
    fn ids_cannot_escape_the_store() {
        let dir=TestDirectory::new();let store=RoomStore::open(dir.0.clone()).unwrap();let mut room=store.create().unwrap();
        for invalid in ["../escape", "..\\escape", "C:\\escape", "/escape", "", "a/b", "AABB"] {
            assert!(store.load(invalid).is_err());room.id=invalid.into();assert!(store.save(&room).is_err());
        }
    }
    #[test]
    fn corrupt_existing_room_is_reported_and_never_overwritten() {
        let dir=TestDirectory::new();let store=RoomStore::open(dir.0.clone()).unwrap();let room=store.create().unwrap();
        let path=dir.0.join(format!("{}.json",room.id));fs::write(&path,b"{broken original").unwrap();
        let error=store.load(&room.id).unwrap_err();assert!(error.contains(&path.display().to_string()));
        assert!(store.list().is_err());assert!(store.save(&room).is_err());
        assert_eq!(fs::read(path).unwrap(),b"{broken original");
    }
    #[test]
    fn invalid_persisted_history_and_summary_metadata_are_preserved() {
        let dir=TestDirectory::new();let store=RoomStore::open(dir.0.clone()).unwrap();let mut valid=store.create().unwrap();
        valid.entries.push(entry(1,"Содержательная реплика для проверки"));store.save(&valid).unwrap();
        let path=dir.0.join(format!("{}.json",valid.id));
        for kind in 0..4 {
            let mut invalid=valid.clone();
            match kind {
                0=>invalid.entries[0].source="unknown".into(),
                1=>invalid.entries[0].at="not-a-date".into(),
                2=>invalid.memory_through=1,
                _=>invalid.summary_through=1,
            }
            let bytes=serde_json::to_vec(&invalid).unwrap();fs::write(&path,&bytes).unwrap();
            assert!(store.load(&valid.id).is_err(),"invalid metadata case {kind}");
            assert!(store.save(&valid).is_err());assert_eq!(fs::read(&path).unwrap(),bytes);
        }
    }
    #[test]
    fn exclusive_store_lock_is_released_on_drop() {
        let dir=TestDirectory::new();let first=RoomStore::open(dir.0.clone()).unwrap();
        assert!(RoomStore::open(dir.0.clone()).is_err());drop(first);assert!(RoomStore::open(dir.0.clone()).is_ok());
    }
    #[test]
    fn create_is_unique_and_pinned_rooms_sort_first() {
        let dir=TestDirectory::new();let store=RoomStore::open(dir.0.clone()).unwrap();
        let mut first=store.create().unwrap();let second=store.create().unwrap();assert_ne!(first.id,second.id);
        first.pinned=true;store.save(&first).unwrap();let rooms=store.list().unwrap();
        assert_eq!(rooms.len(),2);assert_eq!(rooms[0].id,first.id);
    }
    #[test]
    fn backup_keeps_the_previous_valid_version() {
        let dir=TestDirectory::new();let store=RoomStore::open(dir.0.clone()).unwrap();let mut room=store.create().unwrap();
        let backup=dir.0.join(format!("{}.bak",room.id));
        assert!(!backup.exists());
        room.entries.push(entry(1,"Первая сохранённая реплика"));store.save(&room).unwrap();
        let previous:Room=serde_json::from_slice(&fs::read(&backup).unwrap()).unwrap();assert!(previous.entries.is_empty());
        let first=fs::read(dir.0.join(format!("{}.json",room.id))).unwrap();
        room.entries.push(entry(2,"Вторая сохранённая реплика"));store.save(&room).unwrap();
        assert_eq!(fs::read(&backup).unwrap(),first);
        assert_eq!(store.load(&room.id).unwrap().entries.len(),2);
    }
    #[test]
    fn corrupt_backup_prevents_save_and_preserves_both_files() {
        let dir=TestDirectory::new();let store=RoomStore::open(dir.0.clone()).unwrap();let mut room=store.create().unwrap();
        let path=dir.0.join(format!("{}.json",room.id));let before=fs::read(&path).unwrap();
        let backup=dir.0.join(format!("{}.bak",room.id));fs::write(&backup,b"broken backup").unwrap();
        room.entries.push(entry(1,"Несохранённая новая реплика"));
        let error=store.save(&room).unwrap_err();assert!(error.contains(&backup.display().to_string()));
        assert_eq!(fs::read(&path).unwrap(),before);assert_eq!(fs::read(&backup).unwrap(),b"broken backup");
        let (rooms,issues)=store.list_available().unwrap();assert_eq!(rooms.len(),1);assert_eq!(issues.len(),1);
        assert!(!issues[0].available&&issues[0].primary_error.is_none()&&issues[0].backup_error.is_some());
    }
    #[test]
    fn explicit_recovery_restores_previous_version_and_preserves_corrupt_original() {
        let dir=TestDirectory::new();let store=RoomStore::open(dir.0.clone()).unwrap();let mut room=store.create().unwrap();
        room.entries.push(entry(1,"Сохранённая предыдущая реплика"));store.save(&room).unwrap();
        room.entries.push(entry(2,"Последняя реплика перед повреждением"));store.save(&room).unwrap();
        let path=dir.0.join(format!("{}.json",room.id));let backup=path.with_extension("bak");let backup_bytes=fs::read(&backup).unwrap();
        fs::write(&path,b"{broken latest version").unwrap();
        let info=store.recovery_info(&room.id).unwrap();assert!(info.available&&info.primary_error.is_some());
        assert_eq!(info.backup.unwrap().entry_count,1);assert!(info.backup_error.is_none());
        assert_eq!(store.list_recoveries().unwrap().len(),1);assert!(store.load(&room.id).is_err());
        assert_eq!(fs::read(&path).unwrap(),b"{broken latest version");
        let recovered=store.recover(&room.id).unwrap();assert_eq!(recovered.room.entries.len(),1);
        assert_eq!(fs::read(recovered.preserved_path.unwrap()).unwrap(),b"{broken latest version");
        assert!(recovered.preserved_memory_path.is_none());assert_eq!(fs::read(&backup).unwrap(),backup_bytes);
        assert_eq!(fs::read(&path).unwrap(),backup_bytes);assert!(store.recover(&room.id).is_err());
        assert!(store.list_recoveries().unwrap().is_empty());
    }
    #[test]
    fn recovery_detects_missing_primary_and_invalid_backup_without_silent_changes() {
        let dir=TestDirectory::new();let store=RoomStore::open(dir.0.clone()).unwrap();let mut room=store.create().unwrap();
        room.entries.push(entry(1,"Последняя сохранённая реплика"));store.save(&room).unwrap();
        let path=dir.0.join(format!("{}.json",room.id));let backup=path.with_extension("bak");
        fs::remove_file(&path).unwrap();assert!(store.list_recoveries().unwrap()[0].available);
        let recovered=store.recover(&room.id).unwrap();assert!(recovered.preserved_path.is_none());
        fs::write(&backup,b"invalid backup").unwrap();let info=store.recovery_info(&room.id).unwrap();
        assert!(!info.available);assert!(info.primary_error.is_none());assert!(info.backup_error.is_some());
        assert_eq!(store.list_recoveries().unwrap().len(),1);
        fs::write(&path,b"invalid primary").unwrap();assert!(store.recover(&room.id).is_err());
        assert_eq!(fs::read(&path).unwrap(),b"invalid primary");assert_eq!(fs::read(&backup).unwrap(),b"invalid backup");
        for id in ["../escape","","C:\\escape"] { assert!(store.recover(id).is_err());assert!(store.recovery_info(id).is_err()); }
    }
    #[cfg(windows)]
    #[test]
    fn atomic_write_failures_keep_valid_primary_and_backup_and_remove_temporary_files() {
        let dir=TestDirectory::new();let store=RoomStore::open(dir.0.clone()).unwrap();let mut room=store.create().unwrap();
        room.entries.push(entry(1,"Первая реплика"));store.save(&room).unwrap();
        let path=dir.0.join(format!("{}.json",room.id));let backup=path.with_extension("bak");
        let primary_bytes=fs::read(&path).unwrap();let backup_bytes=fs::read(&backup).unwrap();
        room.entries.push(entry(2,"Реплика после ошибки диска"));
        let locked=OpenOptions::new().read(true).share_mode(3).open(&backup).unwrap();
        assert!(store.save(&room).is_err());assert_eq!(fs::read(&path).unwrap(),primary_bytes);assert_eq!(fs::read(&backup).unwrap(),backup_bytes);drop(locked);
        let locked=OpenOptions::new().read(true).share_mode(3).open(&path).unwrap();
        assert!(store.save(&room).is_err());assert_eq!(fs::read(&path).unwrap(),primary_bytes);assert_eq!(fs::read(&backup).unwrap(),primary_bytes);drop(locked);
        assert!(!fs::read_dir(&dir.0).unwrap().any(|e|e.unwrap().path().extension().is_some_and(|x|x=="tmp")));
    }
    #[test]
    fn search_finds_unicode_words_across_fields_and_the_oldest_of_260_entries() {
        let dir=TestDirectory::new();let store=RoomStore::open(dir.0.clone()).unwrap();let mut room=store.create().unwrap();
        room.rename("Проект Ёлка").unwrap();room.summary="Итог: согласован бюджет".into();
        room.entries=(0..260).map(|i|entry(i,"Поздние обычные реплики без искомой темы")).collect();
        room.entries[0].text=format!("{} УНИКАЛЬНЫЙ ДОГОВОР {}", "я🙂 ".repeat(140),"длинный хвост ".repeat(30));
        store.save(&room).unwrap();
        let results=store.search("ЁЛКА бюджет уникальный договор").unwrap();assert_eq!(results.len(),1);
        assert_eq!(results[0].entry_id.as_deref(),Some("0"));assert!(results[0].snippet.contains("УНИКАЛЬНЫЙ ДОГОВОР"));
        assert!(results[0].snippet.chars().count()<=220);assert!(results[0].snippet.starts_with('…'));
        assert!(store.search("ёлка отсутствующий").unwrap().is_empty());
    }
    #[test]
    fn search_orders_pinned_then_recent_and_bounds_unicode_queries() {
        let dir=TestDirectory::new();let store=RoomStore::open(dir.0.clone()).unwrap();
        let mut oldest=store.create().unwrap();oldest.rename("Общая тема").unwrap();oldest.pinned=true;oldest.updated_at="2026-01-01T00:00:00Z".into();store.save(&oldest).unwrap();
        let mut recent=store.create().unwrap();recent.rename("Общая тема").unwrap();recent.updated_at="2026-11-16T00:00:00Z".into();store.save(&recent).unwrap();
        let mut older=store.create().unwrap();older.rename("Общая тема").unwrap();older.updated_at="2026-11-15T00:00:00Z".into();store.save(&older).unwrap();
        let results=store.search("ОбЩаЯ").unwrap();assert_eq!(results.iter().map(|h|h.room.id.as_str()).collect::<Vec<_>>(),vec![oldest.id.as_str(),recent.id.as_str(),older.id.as_str()]);
        assert!(results[0].entry_id.is_none());assert_eq!(store.search(" \n ").unwrap().len(),3);
        assert!(store.search(&"я".repeat(200)).is_ok());assert!(store.search(&"я".repeat(201)).unwrap_err().contains("200"));
        assert!(store.search("a\0b").is_err());
    }
    #[test]
    fn healthy_rooms_stay_searchable_and_every_corrupt_or_invalid_file_is_reported() {
        let dir=TestDirectory::new();let store=RoomStore::open(dir.0.clone()).unwrap();
        let mut healthy=store.create().unwrap();healthy.entries.push(entry(1,"Доступная полезная история"));store.save(&healthy).unwrap();
        let broken=store.create().unwrap();let path=dir.0.join(format!("{}.json",broken.id));fs::write(&path,b"broken without backup").unwrap();
        let invalid=dir.0.join("invalid-name.json");fs::write(&invalid,b"invalid name").unwrap();
        let invalid_backup=dir.0.join("invalid-backup.bak");fs::write(&invalid_backup,b"invalid backup name").unwrap();
        let (rooms,issues)=store.list_available().unwrap();assert_eq!(rooms.len(),1);assert_eq!(rooms[0].id,healthy.id);assert_eq!(issues.len(),3);
        let issue=issues.iter().find(|issue|issue.id==broken.id).unwrap();assert!(!issue.available&&issue.backup.is_none());
        for path in [&path,&invalid,&invalid_backup] {
            assert!(issues.iter().any(|issue|!issue.available&&issue.primary_error.as_ref().or(issue.backup_error.as_ref()).is_some_and(|error|error.contains(&path.display().to_string()))));
        }
        let hits=store.search("полезная").unwrap();assert_eq!(hits.len(),1);assert_eq!(hits[0].room.id,healthy.id);
        assert!(store.list().is_err());assert_eq!(fs::read(path).unwrap(),b"broken without backup");
    }
    #[test]
    fn restoring_one_room_keeps_healthy_rooms_visible_when_another_cannot_be_recovered() {
        let dir=TestDirectory::new();let store=RoomStore::open(dir.0.clone()).unwrap();
        let mut healthy=store.create().unwrap();healthy.entries.push(entry(1,"Полезная история"));store.save(&healthy).unwrap();
        let broken=store.create().unwrap();let broken_path=dir.0.join(format!("{}.json",broken.id));fs::write(&broken_path,b"broken without backup").unwrap();
        let mut recoverable=store.create().unwrap();recoverable.entries.push(entry(1,"Восстановленная история"));store.save(&recoverable).unwrap();
        recoverable.entries.push(entry(2,"Последняя реплика"));store.save(&recoverable).unwrap();
        fs::write(dir.0.join(format!("{}.json",recoverable.id)),b"recoverable corruption").unwrap();
        let (before,issues)=store.list_available().unwrap();assert_eq!(before.len(),1);assert_eq!(issues.len(),2);
        assert!(issues.iter().any(|issue|issue.id==recoverable.id&&issue.available));
        store.recover(&recoverable.id).unwrap();
        let (after,issues)=store.list_available().unwrap();assert_eq!(after.len(),2);assert_eq!(issues.len(),1);assert_eq!(issues[0].id,broken.id);
        assert_eq!(store.search("история").unwrap().len(),2);assert_eq!(fs::read(broken_path).unwrap(),b"broken without backup");
    }
    #[cfg(windows)]
    #[test]
    fn failed_recovery_replacement_keeps_original_backup_and_preserved_copy() {
        let dir=TestDirectory::new();let store=RoomStore::open(dir.0.clone()).unwrap();let mut room=store.create().unwrap();
        room.entries.push(entry(1,"Последняя реплика"));store.save(&room).unwrap();
        let path=dir.0.join(format!("{}.json",room.id));let backup=path.with_extension("bak");let backup_bytes=fs::read(&backup).unwrap();
        fs::write(&path,b"corrupt original").unwrap();
        let locked=OpenOptions::new().read(true).share_mode(3).open(&path).unwrap();
        let error=store.recover(&room.id).unwrap_err();assert!(error.contains("Исходный файл сохранён"));
        assert_eq!(fs::read(&path).unwrap(),b"corrupt original");assert_eq!(fs::read(&backup).unwrap(),backup_bytes);
        let preserved=fs::read_dir(&dir.0).unwrap().filter_map(Result::ok).find(|e|e.path().extension().is_some_and(|ext|ext=="corrupt")).unwrap();
        assert_eq!(fs::read(preserved.path()).unwrap(),b"corrupt original");drop(locked);
        assert!(!fs::read_dir(&dir.0).unwrap().any(|e|e.unwrap().path().extension().is_some_and(|ext|ext=="tmp")));
    }
    #[test]
    fn unsaved_snapshot_disk_failure_is_explicit_and_does_not_modify_primary() {
        let dir=TestDirectory::new();let mut store=RoomStore::open(dir.0.clone()).unwrap();let room=store.create().unwrap();
        let path=dir.0.join(format!("{}.json",room.id));let bytes=fs::read(&path).unwrap();
        store.root=dir.0.join("unavailable-directory");
        assert!(store.preserve_unsaved(&room).unwrap_err().contains("Не удалось создать временный файл"));
        assert_eq!(fs::read(path).unwrap(),bytes);
    }
    #[test]
    fn search_does_not_wait_for_the_store_write_lock() {
        let dir=TestDirectory::new();let store=std::sync::Arc::new(RoomStore::open(dir.0.clone()).unwrap());
        store.create().unwrap();
        let locked=store.access.lock().unwrap();let reader=store.clone();
        let (sender,receiver)=std::sync::mpsc::channel();
        let task=std::thread::spawn(move|| { sender.send((reader.search(""),reader.list_available())).unwrap(); });
        let result=receiver.recv_timeout(std::time::Duration::from_secs(5));
        drop(locked);task.join().unwrap();
        let (hits,available)=result.expect("search and list_available must finish while the write lock is held");
        assert_eq!(hits.unwrap().len(),1);assert_eq!(available.unwrap().0.len(),1);
    }
    #[test]
    fn concurrent_search_and_save_read_complete_consistent_room_versions() {
        let dir=TestDirectory::new();let store=std::sync::Arc::new(RoomStore::open(dir.0.clone()).unwrap());
        let mut room=store.create().unwrap();room.summary="Версия 0".into();
        room.entries=(0..100).map(|i|entry(i,"Версия 0")).collect();store.save(&room).unwrap();
        let barrier=std::sync::Arc::new(std::sync::Barrier::new(2));let writer=store.clone();let start=barrier.clone();
        let task=std::thread::spawn(move|| {
            start.wait();
            for version in 1..=30 {
                room.summary=format!("Версия {version}");
                room.entries=(0..100+version).map(|i|entry(i,&room.summary)).collect();
                writer.save(&room).unwrap();
            }
        });
        barrier.wait();
        for _ in 0..100 {
            let hits=store.search("Версия").unwrap();assert_eq!(hits.len(),1);
            let hit=&hits[0];let version=hit.room.entry_count-100;
            assert_eq!(hit.room.summary,format!("Версия {version}"));assert_eq!(hit.snippet,hit.room.summary);
        }
        task.join().unwrap();assert_eq!(store.search("Версия").unwrap()[0].room.entry_count,130);
    }
}
