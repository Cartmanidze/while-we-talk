use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

#[path = "providers/antigravity.rs"]
mod antigravity;
#[path = "providers/chatgpt.rs"]
mod chatgpt;
#[path = "providers/claude.rs"]
mod claude;

pub type EventSink = Arc<dyn Fn(ProviderEvent) + Send + Sync>;
pub type DeltaSink = Arc<dyn Fn(String) + Send + Sync>;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum ProviderId {
    Chatgpt,
    Antigravity,
    Claude,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProviderEvent {
    AuthUrl { url: String },
    Authenticated { provider: ProviderId },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Answer {
    pub text: String,
    pub model: String,
    pub first_text_seconds: f64,
    pub total_seconds: f64,
    pub usage: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelOption {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub context_window: Option<usize>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Catalog {
    pub models: Vec<ModelOption>,
    pub selected: Option<String>,
    pub connected: bool,
    pub budget_confirmed: bool,
    #[serde(default)]
    pub catalog_source: Option<String>,
}

pub struct Providers {
    http: reqwest::Client,
    chatgpt: Mutex<chatgpt::ChatGpt>,
    antigravity: Mutex<antigravity::Antigravity>,
    claude: Mutex<claude::Claude>,
    work_dir: PathBuf,
    closed: AtomicBool,
}

impl Providers {
    pub fn new(
        registration_dir: PathBuf,
        agy_exe: PathBuf,
        work_dir: PathBuf,
        claude_exe: PathBuf,
    ) -> Result<Self, String> {
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(15))
            .user_agent("LocalSpeechAssistant/0.1")
            .build()
            .map_err(|_| "Не удалось подготовить защищённое соединение".to_string())?;
        Ok(Self {
            http,
            chatgpt: Mutex::new(chatgpt::ChatGpt::new(registration_dir)),
            antigravity: Mutex::new(antigravity::Antigravity::new(agy_exe, work_dir.clone())),
            claude: Mutex::new(claude::Claude::new(claude_exe, work_dir.with_file_name("claude-work"))),
            work_dir,
            closed: AtomicBool::new(false),
        })
    }

    pub async fn connect_chatgpt(
        &self,
        emit: EventSink,
        cancel: Arc<AtomicBool>,
    ) -> Result<Catalog, String> {
        self.ensure_open()?;
        check_cancel(&cancel, &self.closed)?;
        let mut account = self.chatgpt.try_lock().map_err(|_| busy())?;
        account
            .connect(&self.http, emit, &cancel, &self.closed)
            .await?;
        Ok(account.catalog.clone())
    }

    pub async fn confirm_budget(&self, percent: u8) -> Result<Catalog, String> {
        self.ensure_open()?;
        if percent == 0 || percent >= 100 {
            return Err("Лимит приложения должен быть ниже 100%; подтверди его в ChatGPT".into());
        }
        let mut account = self.chatgpt.try_lock().map_err(|_| busy())?;
        account.refresh(&self.http, true).await
    }

    pub async fn refresh_models(&self, provider: ProviderId) -> Result<Catalog, String> {
        self.ensure_open()?;
        match provider {
            ProviderId::Chatgpt => {
                self.chatgpt
                    .try_lock()
                    .map_err(|_| busy())?
                    .refresh(&self.http, false)
                    .await
            }
            ProviderId::Antigravity => {
                self.antigravity
                    .try_lock()
                    .map_err(|_| busy())?
                    .refresh()
                    .await
            }
            ProviderId::Claude => self.claude.try_lock().map_err(|_| busy())?.refresh().await,
        }
    }

    pub async fn select_model(&self, provider: ProviderId, model: &str) -> Result<Catalog, String> {
        self.ensure_open()?;
        match provider {
            ProviderId::Chatgpt => {
                let mut account = self.chatgpt.try_lock().map_err(|_| busy())?;
                if !account.catalog.budget_confirmed {
                    return Err("Подтверди ограничение расходов ChatGPT".into());
                }
                select_catalog(&mut account.catalog, model)?;
                Ok(account.catalog.clone())
            }
            ProviderId::Antigravity => {
                self.antigravity
                    .try_lock()
                    .map_err(|_| busy())?
                    .select(model)
                    .await
            }
            ProviderId::Claude => self.claude.try_lock().map_err(|_| busy())?.select(model).await,
        }
    }

    pub async fn answer(
        &self,
        provider: ProviderId,
        prompt: &str,
        on_delta: DeltaSink,
        cancel: Arc<AtomicBool>,
    ) -> Result<Answer, String> {
        self.ensure_open()?;
        check_cancel(&cancel, &self.closed)?;
        match provider {
            ProviderId::Chatgpt => {
                let mut client = self.chatgpt.try_lock().map_err(|_| busy())?;
                validate_prompt(prompt, selected_window(&client.catalog))?;
                client
                    .answer(&self.http, prompt, on_delta, &cancel, &self.closed)
                    .await
            }
            ProviderId::Antigravity => {
                let mut client = self.antigravity.try_lock().map_err(|_| busy())?;
                validate_prompt(prompt, selected_window(&client.catalog))?;
                client.answer(prompt, on_delta, &cancel, &self.closed).await
            }
            ProviderId::Claude => {
                let mut client = self.claude.try_lock().map_err(|_| busy())?;
                if !client.catalog.connected || !client.catalog.budget_confirmed {
                    return Err("Войди в Claude Code и подтверди отключение дополнительного использования".into());
                }
                validate_prompt(prompt, selected_window(&client.catalog))?;
                client.answer(prompt, on_delta, &cancel, &self.closed).await
            }
        }
    }

    pub async fn reset_context(&self, provider: ProviderId) -> Result<(), String> {
        self.ensure_open()?;
        if provider == ProviderId::Antigravity {
            self.antigravity
                .try_lock()
                .map_err(|_| busy())?
                .dispose()
                .await;
        }
        Ok(())
    }

    pub async fn shutdown(&self) {
        self.closed.store(true, Ordering::Release);
        self.antigravity.lock().await.dispose().await;
        self.claude.lock().await.dispose().await;
        self.chatgpt.lock().await.clear_credentials();
    }

    pub async fn login_claude(&self, cancel: Arc<AtomicBool>) -> Result<Catalog, String> {
        self.ensure_open()?; check_cancel(&cancel, &self.closed)?;
        self.claude.try_lock().map_err(|_| busy())?.login(cancel, &self.closed).await
    }
    pub fn catalog_snapshot(&self, provider: ProviderId) -> Result<Catalog, String> {
        self.ensure_open()?;
        Ok(match provider {
            ProviderId::Chatgpt => self.chatgpt.try_lock().map_err(|_| busy())?.catalog.clone(),
            ProviderId::Antigravity => self.antigravity.try_lock().map_err(|_| busy())?.catalog.clone(),
            ProviderId::Claude => self.claude.try_lock().map_err(|_| busy())?.catalog.clone(),
        })
    }
    pub async fn confirm_claude_subscription(&self) -> Result<Catalog, String> {
        self.ensure_open()?;
        let mut account = self.claude.try_lock().map_err(|_| busy())?;
        account.refresh().await?;
        account.confirm_subscription()
    }
    pub async fn configure_cli(&self, provider: ProviderId, executable: PathBuf) -> Result<(), String> {
        self.ensure_open()?;
        match provider {
            ProviderId::Antigravity => {
                let mut client = self.antigravity.try_lock().map_err(|_| busy())?;
                client.dispose().await;
                *client = antigravity::Antigravity::new(executable, self.work_dir.clone());
            }
            ProviderId::Claude => {
                let mut client = self.claude.try_lock().map_err(|_| busy())?;
                client.dispose().await;
                *client = claude::Claude::new(executable, self.work_dir.with_file_name("claude-work"));
            }
            ProviderId::Chatgpt => return Err("ChatGPT не использует путь к CLI".into()),
        }
        Ok(())
    }

    fn ensure_open(&self) -> Result<(), String> {
        if self.closed.load(Ordering::Acquire) {
            Err("Подключения закрыты".into())
        } else {
            Ok(())
        }
    }
}

fn busy() -> String {
    "Дождись окончания запроса или отмени её".into()
}

// UTF-8 bytes conservatively bound tokens; leave room for provider instructions and output.
pub fn input_budget(window: Option<usize>) -> usize {
    window.unwrap_or(65536).min(2_000_000) * 4 / 5
}
fn selected_window(catalog: &Catalog) -> Option<usize> {
    catalog
        .models
        .iter()
        .find(|m| Some(&m.id) == catalog.selected.as_ref())
        .and_then(|m| m.context_window)
}
fn validate_prompt(prompt: &str, window: Option<usize>) -> Result<(), String> {
    if prompt.trim().is_empty() {
        return Err("Вопрос пустой".into());
    }
    if prompt.len() > input_budget(window) {
        return Err("Контекст превышает рабочее окно модели. Требуется сжатие истории".into());
    }
    Ok(())
}

fn codex_context_window(model: &str) -> Option<usize> {
    let directory = std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(|p| PathBuf::from(p).join(".codex")))?;
    context_window_from_cache(&directory, model)
}
fn context_window_from_cache(directory: &std::path::Path, model: &str) -> Option<usize> {
    let data: Value =
        serde_json::from_slice(&std::fs::read(directory.join("models_cache.json")).ok()?).ok()?;
    let row = data
        .get("models")?
        .as_array()?
        .iter()
        .find(|r| r.get("slug").and_then(Value::as_str) == Some(model))?;
    // A Codex CLI override/max capacity does not prove that the subscription route enabled it.
    let base = row.get("context_window")?.as_u64()?;
    (base > 0).then_some(base.min(2_000_000) as usize)
}
fn check_cancel(cancel: &AtomicBool, closed: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Acquire) || closed.load(Ordering::Acquire) {
        Err("Запрос отменён".into())
    } else {
        Ok(())
    }
}
async fn cancelled(cancel: &AtomicBool, closed: &AtomicBool) {
    while check_cancel(cancel, closed).is_ok() {
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 200
        && id.as_bytes()[0].is_ascii_alphanumeric()
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_.:/-".contains(&c))
}
fn valid_name(name: &str) -> bool {
    !name.trim().is_empty() && name.chars().count() <= 300 && !name.chars().any(char::is_control)
}

fn chatgpt_models(data: &Value) -> Result<Vec<ModelOption>, String> {
    let rows = data
        .get("models")
        .and_then(Value::as_array)
        .ok_or("ChatGPT вернул неверный каталог моделей")?;
    if rows.len() > 2000 {
        return Err("Каталог моделей слишком большой".into());
    }
    let mut models = Vec::new();
    let mut seen = HashSet::new();
    for row in rows {
        if row.get("visibility").and_then(Value::as_str) != Some("list") {
            continue;
        }
        let id = row
            .get("slug")
            .and_then(Value::as_str)
            .ok_or("Неверный идентификатор модели")?;
        let name = row
            .get("display_name")
            .and_then(Value::as_str)
            .unwrap_or(id)
            .trim();
        if !valid_id(id) || !valid_name(name) {
            return Err("Неверное описание модели ChatGPT".into());
        }
        if seen.insert(id.to_string()) {
            models.push(ModelOption {
                id: id.into(),
                name: name.into(),
                context_window: row
                    .get("context_window")
                    .and_then(Value::as_u64)
                    .filter(|n| (1..=2_000_000).contains(n))
                    .map(|n| n as usize)
                    .or_else(|| codex_context_window(id)),
            });
        }
    }
    Ok(models)
}

fn antigravity_models(data: &str) -> Result<Vec<ModelOption>, String> {
    if data.len() > 128000 {
        return Err("Каталог Antigravity слишком большой".into());
    }
    let mut models = Vec::new();
    let mut seen = HashSet::new();
    for line in data.lines().filter(|line| !line.trim().is_empty()) {
        let parts: Vec<_> = line.split('\t').collect();
        if parts.len() != 2 {
            return Err("Antigravity вернул неизвестный формат каталога".into());
        }
        let id = parts[0].trim();
        let name = parts[1].trim();
        if !valid_id(id) || !valid_name(name) {
            return Err("Неверное описание модели Antigravity".into());
        }
        if seen.insert(id.to_string()) {
            models.push(ModelOption {
                id: id.into(),
                name: name.into(),
                context_window: None,
            });
        }
    }
    Ok(models)
}

fn update_catalog(catalog: &mut Catalog, models: Vec<ModelOption>, initial: bool, preferred: &str) {
    let retained = catalog
        .selected
        .as_ref()
        .filter(|id| models.iter().any(|item| &item.id == *id))
        .cloned();
    catalog.selected = retained.or_else(|| {
        if initial {
            models
                .iter()
                .find(|item| item.id == preferred)
                .or_else(|| models.first())
                .map(|item| item.id.clone())
        } else {
            None
        }
    });
    catalog.models = models;
}
fn select_catalog(catalog: &mut Catalog, model: &str) -> Result<(), String> {
    if !catalog.models.iter().any(|item| item.id == model) {
        return Err("Модель отсутствует в доступном каталоге. Обнови список".into());
    }
    catalog.selected = Some(model.into());
    Ok(())
}
fn validate_callback(target: &str, state: &str, client: &str) -> Result<String, String> {
    let url = url::Url::parse(&format!("http://127.0.0.1{target}"))
        .map_err(|_| "Не удалось прочитать ответ страницы входа")?;
    if url.path() != "/auth/callback" {
        return Err("Страница входа вернула неожиданный адрес".into());
    }
    let pairs: Vec<_> = url.query_pairs().collect();
    let states: Vec<_> = pairs.iter().filter(|(k, _)| k == "state").collect();
    if states.len() != 1 || states[0].1 != state {
        return Err("Вход не относится к текущей попытке".into());
    }
    if pairs.iter().any(|(k, _)| k == "error") {
        return Err("Разрешение на вход не получено".into());
    }
    let clients: Vec<_> = pairs.iter().filter(|(k, _)| k == "client_id").collect();
    if clients.len() > 1 || clients.first().is_some_and(|(_, id)| id != client) {
        return Err("Идентификатор приложения изменился".into());
    }
    let codes: Vec<_> = pairs.iter().filter(|(k, _)| k == "code").collect();
    if codes.len() != 1 || codes[0].1.is_empty() || codes[0].1.len() > 8192 {
        return Err("Не получен код входа".into());
    }
    Ok(codes[0].1.to_string())
}

#[derive(Default)]
struct ResponseAccumulator {
    pending: Vec<u8>,
    received: usize,
    text: String,
    completed: bool,
    usage: Value,
}
impl ResponseAccumulator {
    fn push(&mut self, chunk: &[u8]) -> Result<Vec<String>, String> {
        self.received += chunk.len();
        if self.received > 4_000_000 {
            return Err("Ответ модели превысил допустимый размер".into());
        }
        self.pending.extend_from_slice(chunk);
        let mut output = Vec::new();
        while !self.completed {
            let lf = self
                .pending
                .windows(2)
                .position(|part| part == b"\n\n")
                .map(|at| (at, 2));
            let crlf = self
                .pending
                .windows(4)
                .position(|part| part == b"\r\n\r\n")
                .map(|at| (at, 4));
            let Some((position, length)) = lf.into_iter().chain(crlf).min_by_key(|(at, _)| *at)
            else {
                break;
            };
            let block = String::from_utf8(self.pending.drain(..position).collect())
                .map_err(|_| "Ответ содержит неверный UTF-8")?;
            self.pending.drain(..length);
            let data = block
                .lines()
                .filter_map(|line| line.strip_prefix("data:"))
                .map(str::trim_start)
                .collect::<Vec<_>>()
                .join("\n");
            if data.is_empty() || data == "[DONE]" {
                continue;
            }
            let event: Value =
                serde_json::from_str(&data).map_err(|_| "Неверный формат потока ответа")?;
            match event.get("type").and_then(Value::as_str) {
                Some("response.output_text.delta") => {
                    let delta = event
                        .get("delta")
                        .and_then(Value::as_str)
                        .ok_or("Неверный фрагмент ответа")?;
                    self.text.push_str(delta);
                    if self.text.len() > 200000 {
                        return Err("Ответ модели слишком длинный".into());
                    }
                    if !delta.is_empty() {
                        output.push(delta.into());
                    }
                }
                Some("response.failed" | "response.incomplete" | "error") => {
                    return Err(service_error(&event))
                }
                Some("response.completed") => {
                    if event.pointer("/response/status").and_then(Value::as_str)
                        != Some("completed")
                    {
                        return Err("Ответ не завершён".into());
                    }
                    self.completed = true;
                    self.usage =
                        usage_numbers(event.pointer("/response/usage").unwrap_or(&Value::Null));
                }
                _ => {}
            }
        }
        if self.pending.len() > 1_000_000 {
            return Err("Фрагмент ответа слишком большой".into());
        }
        Ok(output)
    }
    fn finish(self) -> Result<(String, Value), String> {
        if !self.completed || self.text.trim().is_empty() {
            Err("Поток оборвался без подтверждённого ответа".into())
        } else {
            Ok((self.text, self.usage))
        }
    }
}

fn usage_numbers(value: &Value) -> Value {
    let mut output = serde_json::Map::new();
    if let Some(object) = value.as_object() {
        for (key, value) in object {
            if key.ends_with("_tokens") && value.is_number() {
                output.insert(key.clone(), value.clone());
            }
        }
    }
    Value::Object(output)
}
fn service_error(value: &Value) -> String {
    let code = value
        .pointer("/error/code")
        .or_else(|| value.pointer("/response/error/code"))
        .and_then(Value::as_str)
        .unwrap_or("");
    match code {
        "subscription_sharing_usage_limit_exceeded" => {
            "Достигнут лимит подписки или приложения".into()
        }
        "subscription_sharing_usage_unavailable" => {
            "Использование подписки сейчас недоступно".into()
        }
        _ => "Сервис не завершил ответ. Проверь выбранную модель, вход и доступную квоту".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn model(id: &str) -> ModelOption {
        ModelOption {
            id: id.into(),
            name: id.into(),
            context_window: None,
        }
    }

    #[test]
    fn known_small_context_window_is_never_inflated() {
        assert_eq!(input_budget(Some(4096)), 3276);
        let models=chatgpt_models(&json!({"models":[{"slug":"test-small-context","visibility":"list","context_window":4096}]})).unwrap();
        assert_eq!(models[0].context_window, Some(4096));
    }

    #[test]
    fn cli_configuration_never_expands_a_subscription_model_context() {
        let path = std::env::temp_dir().join(format!(
            "sobesednik-context-cache-{:032x}",
            rand::random::<u128>()
        ));
        std::fs::create_dir(&path).unwrap();
        std::fs::write(
            path.join("models_cache.json"),
            json!({"models":[
                {"slug":"wanted","context_window":272000,"max_context_window":872000},
                {"slug":"other","context_window":500000,"max_context_window":872000},
                {"slug":"small","context_window":4096,"max_context_window":872000}
            ]})
            .to_string(),
        )
        .unwrap();
        std::fs::write(
            path.join("config.toml"),
            "model = \"other\"\nmodel_context_window = 872000\n",
        )
        .unwrap();
        let actual = [
            context_window_from_cache(&path, "wanted"),
            context_window_from_cache(&path, "other"),
            context_window_from_cache(&path, "small"),
            context_window_from_cache(&path, "absent"),
        ];
        std::fs::remove_file(path.join("models_cache.json")).unwrap();
        std::fs::remove_file(path.join("config.toml")).unwrap();
        std::fs::remove_dir(path).unwrap();
        assert_eq!(actual, [Some(272000), Some(500000), Some(4096), None]);
    }

    #[test]
    fn long_context_uses_model_budget_instead_of_old_character_limit() {
        assert!(validate_prompt(&"я".repeat(30000), Some(272000)).is_ok());
        assert!(validate_prompt(&"я".repeat(30000), Some(8192)).is_err());
        assert!(validate_prompt("   ", Some(272000)).is_err());
        assert_eq!(input_budget(Some(872000)), 697600);
        assert!(input_budget(None) >= 32000);
    }

    #[tokio::test]
    async fn cancelled_login_does_not_read_registration_or_open_browser() {
        let providers = Providers::new(
            PathBuf::from("missing-test-registration"),
            PathBuf::from("missing-test-cli"),
            PathBuf::from("unused"),
            PathBuf::from("missing-test-claude"),
        )
        .unwrap();
        let emitted = Arc::new(AtomicBool::new(false));
        let flag = emitted.clone();
        let error = providers
            .connect_chatgpt(
                Arc::new(move |_| {
                    flag.store(true, Ordering::SeqCst);
                }),
                Arc::new(AtomicBool::new(true)),
            )
            .await
            .unwrap_err();
        assert!(error.contains("отменён"), "{error}");
        assert!(!emitted.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn busy_provider_rejects_model_changes_without_waiting() {
        let providers = Providers::new(PathBuf::new(), PathBuf::new(), PathBuf::new(), PathBuf::new()).unwrap();
        let _held = providers.antigravity.lock().await;
        let answer = tokio::time::timeout(
            Duration::from_millis(200),
            providers.select_model(ProviderId::Antigravity, "chosen"),
        )
        .await;
        assert!(answer.unwrap().unwrap_err().contains("Дождись"));
        assert!(providers.confirm_budget(100).await.is_err());
    }

    #[test]
    fn account_catalog_keeps_all_visible_models_in_order() {
        let models = chatgpt_models(&json!({"models": [
            {"slug":"gpt-astra","display_name":"Astra","visibility":"list"},
            {"slug":"gpt-terra","display_name":"Terra","visibility":"list"},
            {"slug":"gpt-astra","display_name":"Duplicate","visibility":"list"},
            {"slug":"hidden","visibility":"hide"}
        ]}))
        .unwrap();
        assert_eq!(
            models,
            vec![
                ModelOption {
                    id: "gpt-astra".into(),
                    name: "Astra".into(),
                    context_window: None,
                },
                ModelOption {
                    id: "gpt-terra".into(),
                    name: "Terra".into(),
                    context_window: None,
                }
            ]
        );
        assert!(
            chatgpt_models(&json!({"models":[{"slug":"bad\nmodel","visibility":"list"}]})).is_err()
        );
    }

    #[test]
    fn cli_catalog_validates_and_deduplicates_tsv() {
        let models =
            antigravity_models("gemini-a\tGemini A\r\nclaude-b\tClaude B\nclaude-b\tDuplicate\n")
                .unwrap();
        assert_eq!(models.len(), 2);
        assert_eq!(models[1].id, "claude-b");
        assert!(antigravity_models("not a catalog").is_err());
        assert!(antigravity_models("id\tname\textra").is_err());
        assert!(antigravity_models("").unwrap().is_empty());
    }

    #[test]
    fn removed_model_never_silently_falls_back() {
        let mut catalog = Catalog::default();
        update_catalog(
            &mut catalog,
            vec![model("fast"), model("quality")],
            true,
            "fast",
        );
        assert_eq!(catalog.selected.as_deref(), Some("fast"));
        select_catalog(&mut catalog, "quality").unwrap();
        update_catalog(&mut catalog, vec![model("fast")], false, "fast");
        assert_eq!(catalog.selected, None);
        assert!(select_catalog(&mut catalog, "missing").is_err());
        select_catalog(&mut catalog, "fast").unwrap();
        assert_eq!(catalog.selected.as_deref(), Some("fast"));
    }

    #[test]
    fn callback_binds_state_and_registered_client() {
        assert_eq!(
            validate_callback(
                "/auth/callback?state=expected&code=test&client_id=oaiapp_test",
                "expected",
                "oaiapp_test"
            )
            .unwrap(),
            "test"
        );
        for target in [
            "/auth/callback?state=wrong&code=test",
            "/auth/callback?state=expected&state=expected&code=test",
            "/auth/callback?state=expected&code=test&client_id=other",
            "/auth/callback?state=expected&error=access_denied",
            "/other?state=expected&code=test",
        ] {
            assert!(validate_callback(target, "expected", "oaiapp_test").is_err());
        }
    }

    #[test]
    fn sse_reassembles_fragmented_unicode_and_requires_completion() {
        let source = concat!("data: {\"type\":\"response.output_text.delta\",\"delta\":\"Привет\"}\r\n\r\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"usage\":{\"total_tokens\":3}}}\r\n\r\n");
        let mut parser = ResponseAccumulator::default();
        let mut output = String::new();
        for byte in source.as_bytes() {
            for delta in parser.push(&[*byte]).unwrap() {
                output.push_str(&delta);
            }
        }
        assert_eq!(output, "Привет");
        let (text, usage) = parser.finish().unwrap();
        assert_eq!(text, "Привет");
        assert_eq!(usage["total_tokens"], 3);
        let mut incomplete = ResponseAccumulator::default();
        incomplete
            .push(b"data: {\"type\":\"response.output_text.delta\",\"delta\":\"partial\"}\n\n")
            .unwrap();
        assert!(incomplete.finish().is_err());
    }

    #[test]
    fn failure_after_partial_output_is_not_success() {
        let mut parser = ResponseAccumulator::default();
        parser
            .push(b"data: {\"type\":\"response.output_text.delta\",\"delta\":\"partial\"}\n\n")
            .unwrap();
        assert!(parser.push(b"data: {\"type\":\"response.failed\",\"response\":{\"error\":{\"code\":\"usage_limit\"}}}\n\n").is_err());
    }

    #[test]
    fn completed_sse_preserves_code_indentation_and_trailing_newlines() {
        let original = "    code();\n\n";
        let mut parser = ResponseAccumulator::default();
        let chunk = format!(
            "data: {}\n\ndata: {}\n\n",
            json!({"type":"response.output_text.delta","delta":original}),
            json!({"type":"response.completed","response":{"status":"completed"}})
        );
        assert_eq!(
            parser.push(chunk.as_bytes()).unwrap(),
            vec![original.to_string()]
        );
        assert_eq!(parser.finish().unwrap().0, original);
    }
}
