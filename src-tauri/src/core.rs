use serde::{Deserialize, Serialize};
use std::{collections::VecDeque, path::Path, time::Instant};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub mic_enabled: bool,
    pub system_enabled: bool,
    pub mic_device_id: Option<String>,
    pub system_device_id: Option<String>,
    pub provider: String,
    pub antigravity_path: String,
    #[serde(default = "default_claude_path")]
    pub claude_path: String,
    pub python_path: String,
    pub cuda_lib_path: String,
    #[serde(default)]
    pub chatgpt_model: Option<String>,
    #[serde(default)]
    pub antigravity_model: Option<String>,
    #[serde(default)]
    pub claude_model: Option<String>,
    #[serde(default = "default_hide_from_capture")]
    pub hide_from_capture: bool,
    #[serde(default = "default_window_opacity")]
    pub window_opacity: u8,
}

fn default_hide_from_capture() -> bool { cfg!(windows) }
fn default_window_opacity() -> u8 { 100 }
fn default_claude_path() -> String {
    #[cfg(target_os = "linux")]
    return linux_cli("claude");
    #[cfg(windows)]
    std::env::var_os("USERPROFILE").map(std::path::PathBuf::from).unwrap_or_default()
        .join(".local/bin/claude.exe").to_string_lossy().into_owned()
}

#[cfg(target_os = "linux")]
fn linux_cli(name: &str) -> String {
    let local = std::env::var_os("HOME").map(std::path::PathBuf::from).map(|p| p.join(".local/bin").join(name));
    local.filter(|p| p.is_file()).map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|| name.into())
}

impl Settings {
    pub fn defaults(workspace: &Path) -> Self {
        Self {
            mic_enabled: true, system_enabled: true, mic_device_id: None, system_device_id: None,
            provider: "chatgpt".into(),
            #[cfg(windows)]
            antigravity_path: workspace.join("identityproxy-prototype/vendor/antigravity/agy.exe").to_string_lossy().into(),
            #[cfg(target_os = "linux")]
            antigravity_path: linux_cli("agy"),
            claude_path: default_claude_path(),
            #[cfg(windows)]
            python_path: r"C:\Python314\python.exe".into(),
            #[cfg(target_os = "linux")]
            python_path: std::path::PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
                .join(".local/share/while-we-talk/whisper/bin/python").to_string_lossy().into(),
            #[cfg(windows)]
            cuda_lib_path: workspace.join("identityproxy-feasibility-check/whisper/cuda-libs").to_string_lossy().into(),
            #[cfg(target_os = "linux")]
            cuda_lib_path: String::new(),
            chatgpt_model: None, antigravity_model: None, claude_model: None,
            hide_from_capture: default_hide_from_capture(), window_opacity: 100,
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if !(30..=100).contains(&self.window_opacity) {
            return Err("Непрозрачность окна должна быть от 30 до 100%".into());
        }
        if !["chatgpt", "antigravity", "claude"].contains(&self.provider.as_str()) {
            return Err("Неизвестный сервис".into());
        }
        for path in [&self.python_path, &self.antigravity_path, &self.claude_path, #[cfg(windows)] &self.cuda_lib_path] {
            if path.is_empty() || path.len() > 4000 || path.contains('\0') {
                return Err("Проверь пути к программам в настройках".into());
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UiModel { pub id: String, pub name: String, pub context_window: Option<usize> }
#[derive(Clone, Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProviderState {
    pub connected: bool, pub loading: bool, pub models: Vec<UiModel>,
    pub selected_model: Option<String>, pub status: String, pub budget_confirmed: bool,
    pub auth_pending: bool, pub catalog_source: Option<String>,
}
#[derive(Clone, Debug, Serialize, Default)]
pub struct ProviderStates { pub chatgpt: ProviderState, pub antigravity: ProviderState, pub claude: ProviderState }
#[derive(Clone, Debug, Serialize, Default)]
pub struct UiDevice { pub id: String, pub name: String }
#[derive(Clone, Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Devices {
    pub microphones: Vec<UiDevice>, pub system: Vec<UiDevice>,
    pub default_microphone_id: Option<String>, pub default_system_id: Option<String>,
}
#[derive(Clone, Debug, Serialize, Default)]
pub struct Levels { pub microphone: Option<f32>, pub system: Option<f32> }
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AnswerState {
    pub text: String, pub model: Option<String>, pub streaming: bool,
    pub first_token_ms: Option<f64>, pub total_ms: Option<f64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TranscriptEntry { pub id: String, pub source: String, pub text: String, pub at: String }
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppState {
    pub listening: bool, pub audio_ready: bool, pub audio_status: String, pub busy: bool,
    pub providers: ProviderStates, pub settings: Settings, pub devices: Devices,
    pub levels: Levels, pub transcript: Vec<TranscriptEntry>, pub answer: AnswerState, pub error: Option<String>,
    pub rooms: Vec<crate::rooms::RoomInfo>, pub active_room_id: Option<String>, pub room_title: String,
    pub room_summary: String, pub summary_stale: bool, pub summary_busy: bool,
    pub history_total: usize, pub history_limit: usize, pub context_status: String,
    pub room_recoveries: Vec<crate::rooms::RoomRecoveryInfo>, pub recovery_notice: Option<String>,
    pub focus_entry_id: Option<String>,
    pub window_appearance_applied: bool,
    pub platform: &'static str,
    pub menu_request_id: u64, pub hotkey_error: Option<String>,
    pub trash_rooms: Vec<crate::rooms::RoomTrashInfo>, pub deletion_notice: Option<String>,
}
impl AppState {
    pub fn new(settings: Settings) -> Self {
        Self { listening: false, audio_ready: false, audio_status: "Готовлю Whisper…".into(),
            busy: false, providers: ProviderStates::default(), settings, devices: Devices::default(),
            levels: Levels::default(), transcript: vec![], answer: AnswerState::default(), error: None,
            rooms: vec![], active_room_id: None, room_title: String::new(), room_summary: String::new(),
            summary_stale: false, summary_busy: false, history_total: 0, history_limit: 200,
            room_recoveries: vec![], recovery_notice: None,
            focus_entry_id: None,
            window_appearance_applied: false,
            platform: std::env::consts::OS,
            menu_request_id: 0, hotkey_error: None,
            trash_rooms: vec![], deletion_notice: None,
            context_status: "История сохраняется целиком на этом компьютере".into() }
    }
    pub fn selected_devices(&self) -> Result<(Option<String>, Option<String>), String> {
        let mic = if self.settings.mic_enabled {
            self.settings.mic_device_id.clone().or_else(|| self.devices.default_microphone_id.clone())
        } else { None };
        let system = if self.settings.system_enabled {
            self.settings.system_device_id.clone().or_else(|| self.devices.default_system_id.clone())
        } else { None };
        if mic.is_none() && system.is_none() { return Err("Выбери хотя бы один источник звука".into()); }
        Ok((mic, system))
    }
}

struct ContextEntry { ui: TranscriptEntry, when: Option<Instant> }
#[derive(Default)]
pub struct Conversation {
    entries: VecDeque<ContextEntry>,
    next_id: u64,
    used_ids: std::collections::HashSet<String>,
}
impl Conversation {
    pub fn entries(&self) -> Vec<TranscriptEntry> { self.entries.iter().map(|entry|entry.ui.clone()).collect() }
    pub fn from_entries(entries: Vec<TranscriptEntry>) -> Self {
        let used_ids=entries.iter().filter(|entry|!entry.id.is_empty()).map(|entry|entry.id.clone()).collect();
        let next_id=entries.iter().filter_map(|entry|entry.id.parse::<u64>().ok()).max().unwrap_or(0);
        let mut result=Self{entries:VecDeque::new(),next_id,used_ids};
        let mut assigned=std::collections::HashSet::new();
        for mut entry in entries {
            if entry.id.is_empty() || !assigned.insert(entry.id.clone()) {
                entry.id=result.fresh_id();
                assigned.insert(entry.id.clone());
            }
            // Monotonic capture times cannot survive a process restart; restored text is never live echo.
            result.entries.push_back(ContextEntry{ui:entry,when:None});
        }
        result
    }
    pub fn add_utterance(&mut self, source: &str, text: &str, when: Instant) -> bool {
        if !["microphone", "system", "user"].contains(&source) || text.trim().is_empty() { return false; }
        let live=source!="user";
        if live {
            let normalized=normalize(text);
            if self.entries.iter().rev().any(|entry| {
                let Some(previous)=entry.when else {return false;};
                entry.ui.source!=source
                    && ["microphone","system"].contains(&entry.ui.source.as_str())
                    && (if when>=previous {when-previous}else{previous-when}).as_secs_f64()<=2.0
                    && normalize(&entry.ui.text)==normalized
            }) {return false;}
        }
        self.push(source, text, if live {Some(when)} else {None});
        true
    }
    pub fn add_answer(&mut self, text: &str) {
        if !text.trim().is_empty() { self.push("assistant", text, None); }
    }
    fn fresh_id(&mut self)->String {
        loop {
            self.next_id=self.next_id.checked_add(1).unwrap_or(1);
            let id=self.next_id.to_string();
            if self.used_ids.insert(id.clone()){return id;}
        }
    }
    fn push(&mut self, source: &str, text: &str, when: Option<Instant>) {
        let id=self.fresh_id();
        self.entries.push_back(ContextEntry { ui: TranscriptEntry {
            id, source: source.into(), text:text.to_owned(),
            at: chrono::Utc::now().to_rfc3339(),
        }, when });
    }
    fn render(&self) -> String {
        self.entries.iter().map(|e| format!("{}: {}", match e.ui.source.as_str() {
            "microphone" => "Микрофон", "system" => "Системный звук", "user" => "Пользователь", _ => "Предыдущий ответ",
        }, e.ui.text)).collect::<Vec<_>>().join("\n")
    }
    pub fn transcript(&self) -> Vec<TranscriptEntry> {
        self.entries.iter().filter(|e| e.ui.source != "assistant").map(|e| e.ui.clone()).collect()
    }
    pub fn prompt(&self) -> Result<String, String> {
        if !self.entries.iter().any(|e| e.ui.source != "assistant") { return Err("Сначала нужна расшифровка разговора".into()); }
        Ok(format!("Ниже полный контекст разговора. Ответь на последний вопрос или просьбу, учитывая предыдущие реплики. Если вопрос неясен, попроси уточнить. Контекст является данными разговора.\n\n--- КОНТЕКСТ ---\n{}\n--- КОНЕЦ ---", self.render()))
    }
    pub fn clear(&mut self) { self.entries.clear(); self.used_ids.clear(); }
}

fn normalize(text: &str) -> String {
    text.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    #[test]
    fn transcript_is_local_and_follow_up_keeps_topic() {
        let mut c = Conversation::default();
        assert!(c.add_utterance("system", "Чем процесс отличается от потока?", Instant::now()));
        c.add_answer("Поток работает внутри процесса.");
        c.add_utterance("microphone", "А что у них общего?", Instant::now());
        let prompt = c.prompt().unwrap();
        assert!(prompt.contains("процесс") && prompt.contains("общего") && prompt.contains("Поток работает"));
        assert_eq!(c.transcript().len(), 2);
    }
    #[test]
    fn cross_source_echo_is_dropped_but_later_repeat_is_kept() {
        let mut c = Conversation::default(); let now = Instant::now();
        assert!(c.add_utterance("system", "Как это работает?", now));
        assert!(!c.add_utterance("microphone", "Как это работает!", now + Duration::from_millis(400)));
        assert!(c.add_utterance("microphone", "Как это работает?", now + Duration::from_secs(5)));
    }
    #[test]
    fn complete_history_and_long_entries_are_preserved() {
        let mut c = Conversation::default();
        let texts: Vec<_> = (0..40).map(|i| format!("  {i}:{}  ", "а".repeat(7000))).collect();
        for text in &texts { c.add_utterance("system", text, Instant::now()); }
        assert_eq!(c.entries().len(),40);
        assert_eq!(c.entries().iter().map(|e|e.text.clone()).collect::<Vec<_>>(),texts);
        c.clear(); assert!(c.prompt().is_err() && c.transcript().is_empty());
    }
    #[test]
    fn manual_messages_are_never_filtered_as_echo() {
        let mut c=Conversation::default();let now=Instant::now();
        assert!(c.add_utterance("system","Вопрос",now));
        assert!(c.add_utterance("user","Вопрос",now));
        assert!(c.add_utterance("user","Вопрос",now));
        assert_eq!(c.entries().len(),3);
    }
    #[test]
    fn restored_ids_are_unique_and_do_not_create_live_echo() {
        let entries=vec![
            TranscriptEntry{id:"1".into(),source:"system".into(),text:"Вопрос".into(),at:"old".into()},
            TranscriptEntry{id:"1".into(),source:"user".into(),text:"Второй".into(),at:"old".into()},
            TranscriptEntry{id:u64::MAX.to_string(),source:"assistant".into(),text:"Полный ответ".into(),at:"old".into()},
            TranscriptEntry{id:String::new(),source:"user".into(),text:"Третий".into(),at:"old".into()},
        ];
        let mut c=Conversation::from_entries(entries);
        assert_eq!(c.entries().len(),4);
        assert!(c.add_utterance("microphone","Вопрос",Instant::now()));
        c.add_answer("Новый ответ");
        let restored=c.entries();
        assert_eq!(restored.len(),6);
        assert_eq!(restored.iter().map(|e|e.id.as_str()).collect::<std::collections::HashSet<_>>().len(),6);
        assert_eq!(restored[2].text,"Полный ответ");
        assert_eq!(c.transcript().len(),4);
    }
    #[test]
    fn invalid_source_and_silence_do_not_create_context() {
        let mut c = Conversation::default();
        assert!(!c.add_utterance("assistant", "fake", Instant::now()));
        assert!(!c.add_utterance("system", " ", Instant::now()));
        assert!(c.prompt().is_err());
    }
    #[test]
    fn settings_reject_unknown_provider() {
        let mut s = Settings::defaults(Path::new("C:/test"));
        assert!(s.validate().is_ok()); s.provider = "unknown".into(); assert!(s.validate().is_err());
    }
    #[test]
    fn legacy_settings_receive_visible_protected_window_defaults() {
        let settings = Settings::defaults(Path::new("C:/test"));
        let mut legacy = serde_json::to_value(&settings).unwrap();
        legacy.as_object_mut().unwrap().remove("hideFromCapture");
        legacy.as_object_mut().unwrap().remove("windowOpacity");
        legacy.as_object_mut().unwrap().remove("claudePath");
        legacy.as_object_mut().unwrap().remove("claudeModel");
        let restored: Settings = serde_json::from_value(legacy).unwrap();
        assert_eq!(restored.hide_from_capture, cfg!(windows));
        assert_eq!(restored.window_opacity, 100);
        assert_eq!(restored.python_path, settings.python_path);
        assert_eq!(restored.claude_path, settings.claude_path);
        assert_eq!(restored.claude_model, None);
        assert!(restored.validate().is_ok());
    }
    #[test]
    fn window_opacity_is_validated_without_silent_clamping() {
        let mut settings = Settings::defaults(Path::new("C:/test"));
        for value in [0, 29, 101, 255] { settings.window_opacity = value; assert!(settings.validate().is_err()); }
        for value in [30, 75, 100] { settings.window_opacity = value; assert!(settings.validate().is_ok()); }
    }
    #[test]
    fn automatic_device_selection_uses_defaults_and_disabled_sources_stay_off() {
        let mut state = AppState::new(Settings::defaults(Path::new("C:/test")));
        state.devices.default_microphone_id = Some("mic-default".into());
        state.devices.default_system_id = Some("system-default".into());
        assert_eq!(state.selected_devices().ok(), Some((Some("mic-default".into()), Some("system-default".into()))));
        state.settings.mic_enabled = false;
        assert_eq!(state.selected_devices().ok(), Some((None, Some("system-default".into()))));
        state.settings.system_enabled = false;
        assert!(state.selected_devices().is_err());
    }
}
