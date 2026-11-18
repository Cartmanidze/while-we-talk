mod audio;
mod core;
mod providers;
mod transcription;
mod rooms;
mod session;
mod context;
mod window_appearance;
mod hotkeys;

use crate::core::*;
use providers::{Catalog, ProviderEvent, ProviderId, Providers};
use std::{collections::HashMap, fs, path::{Path, PathBuf}, sync::{Arc, Mutex, atomic::{AtomicBool, AtomicU64, Ordering}}, time::Instant};
use tauri::{Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;
use transcription::Transcriber;
use session::RoomSession;
use tauri_plugin_global_shortcut::GlobalShortcutExt;

struct Backend {
    view: Mutex<AppState>,
    emit_lock: Mutex<()>,
    session: Mutex<RoomSession>,
    providers: Arc<Providers>,
    transcriber: Mutex<Option<Transcriber>>,
    capture_stop: Mutex<Option<audio::CaptureStop>>,
    capture_cancel: Mutex<Option<Arc<AtomicBool>>>,
    answer_cancel: Mutex<Arc<AtomicBool>>,
    login_cancel: Mutex<HashMap<ProviderId, Arc<AtomicBool>>>,
    closing: Arc<AtomicBool>,
    close_requested: AtomicBool,
    menu_requests: AtomicU64,
    audio_generation: AtomicU64,
    audio_initializing: AtomicBool,
    audio_init_threads: Mutex<Vec<std::thread::JoinHandle<()>>>,
    settings_file: PathBuf,
    settings_lock: Mutex<()>,
    worker_path: PathBuf,
}

impl Backend {
    fn snapshot(&self) -> AppState { self.view.lock().unwrap().clone() }
    fn emit(&self, app: &tauri::AppHandle) {
        let _ordered = self.emit_lock.lock().unwrap();
        if !self.closing.load(Ordering::Acquire) { let _ = app.emit("app-state", self.snapshot()); }
    }
    fn clients(&self) -> Arc<Providers> { self.providers.clone() }
    fn sync_room(&self, session: &RoomSession) {
        let mut view = self.view.lock().unwrap();
        if let Some(room) = session.room.as_ref() {
            view.active_room_id = Some(room.id.clone());
            view.room_title = room.title.clone();
            view.room_summary = room.summary.clone();
            view.summary_stale = !room.summary.trim().is_empty() && room.summary_through < room.entries.len();
            view.history_total = room.entries.len();
            let start = room.entries.len().saturating_sub(view.history_limit);
            view.transcript = room.entries[start..].to_vec();
            view.rooms.retain(|r| r.id != room.id);
            view.rooms.push(room.info());
            view.rooms.sort_by(|a, b| b.pinned.cmp(&a.pinned).then(b.updated_at.cmp(&a.updated_at)));
        } else {
            view.active_room_id = None; view.room_title.clear(); view.room_summary.clear();
            view.summary_stale = false; view.history_total = 0; view.history_limit = 200;
            view.transcript.clear(); view.answer = AnswerState::default(); view.focus_entry_id = None;
        }
    }
    fn sync_saved_room<T>(&self, session: &RoomSession, result: Result<T, String>) -> Result<T, String> {
        self.sync_room(session);
        if result.is_err() {
            if let Ok((_, recoveries)) = session.store.list_available() {
                self.view.lock().unwrap().room_recoveries = recoveries;
            }
        }
        result
    }
    fn refresh_room_lists(&self, session: &RoomSession) {
        let rooms = session.store.list_available();
        let trash = session.store.trash_list();
        let mut view = self.view.lock().unwrap();
        let mut errors = Vec::new();
        match rooms { Ok((rooms, recoveries)) => { view.rooms = rooms; view.room_recoveries = recoveries; }, Err(error) => errors.push(error) }
        match trash { Ok(rooms) => view.trash_rooms = rooms, Err(error) => errors.push(error) }
        if !errors.is_empty() { view.error = Some(errors.join("\n")); }
    }
    fn begin_room_change(&self) -> Result<(), String> {
        let mut view = self.view.lock().unwrap();
        if view.listening || view.busy || self.close_requested.load(Ordering::Acquire) || self.closing.load(Ordering::Acquire) {
            return Err("Останови расшифровку и дождись окончания запроса".into());
        }
        view.busy = true; view.error = None;
        Ok(())
    }
    fn fail(&self, app: &tauri::AppHandle, message: String) {
        self.view.lock().unwrap().error = Some(message);
        self.emit(app);
    }
    // The caller holds settings_lock from the settings mutation through persistence or rollback.
    fn persist_unlocked(&self) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(&self.view.lock().unwrap().settings).map_err(|_| "Не удалось подготовить настройки")?;
        let temp = self.settings_file.with_extension("tmp");
        fs::write(&temp, bytes).map_err(|_| "Не удалось сохранить настройки")?;
        fs::rename(temp, &self.settings_file).map_err(|_| "Не удалось заменить файл настроек".to_string())
    }
    fn provider_ui(view: &mut AppState, provider: ProviderId) -> &mut ProviderState {
        match provider { ProviderId::Chatgpt => &mut view.providers.chatgpt, ProviderId::Antigravity => &mut view.providers.antigravity, ProviderId::Claude => &mut view.providers.claude }
    }
    fn begin_provider(&self, provider: ProviderId) -> Result<(), String> {
        let mut view = self.view.lock().unwrap();
        if view.busy || self.close_requested.load(Ordering::Acquire) || self.closing.load(Ordering::Acquire) { return Err("Дождись ответа или отмени его".into()); }
        let state = Self::provider_ui(&mut view, provider);
        if state.loading { return Err("Подключение уже обновляется".into()); }
        state.loading = true; state.status = "Проверяю подключение…".into();
        view.error = None;
        Ok(())
    }
    fn apply_catalog(&self, provider: ProviderId, catalog: Catalog) {
        let mut view = self.view.lock().unwrap();
        let state = Self::provider_ui(&mut view, provider);
        state.models = catalog.models.into_iter().map(|m| UiModel { id: m.id, name: m.name, context_window: m.context_window }).collect();
        state.selected_model = catalog.selected;
        state.connected = catalog.connected;
        state.budget_confirmed = catalog.budget_confirmed;
        state.catalog_source = catalog.catalog_source;
        state.loading = false; state.auth_pending = false;
        state.status = if !state.connected { "Нужно войти".into() }
        else if !state.budget_confirmed && provider != ProviderId::Antigravity { "Ты вошёл в аккаунт. Проверь лимиты подписки.".into() }
        else if !state.models.is_empty() {
            format!("Моделей: {} · {}", state.models.len(), if state.selected_model.is_some() { "готово" } else { "выбери модель" })
        } else if state.connected { "Подключение есть. Обнови список моделей.".into() }
        else { "Подключи сервис".into() };
    }
    fn provider_failed(&self, app: &tauri::AppHandle, provider: ProviderId, error: String) {
        if let Ok(catalog) = self.clients().catalog_snapshot(provider) { self.apply_catalog(provider, catalog); }
        let mut view = self.view.lock().unwrap();
        let p = Self::provider_ui(&mut view, provider);
        p.loading = false; p.status = error.clone(); view.error = Some(error);
        drop(view); self.emit(app);
    }
    fn begin_login(&self, provider: ProviderId) -> Result<Arc<AtomicBool>, String> {
        self.begin_provider(provider)?;
        let cancel = Arc::new(AtomicBool::new(self.closing.load(Ordering::Acquire) || self.close_requested.load(Ordering::Acquire)));
        self.login_cancel.lock().unwrap().insert(provider, cancel.clone());
        Self::provider_ui(&mut self.view.lock().unwrap(), provider).auth_pending = true;
        Ok(cancel)
    }
    fn finish_login(&self, app: &tauri::AppHandle, provider: ProviderId, cancel: &Arc<AtomicBool>, result: Result<Catalog, String>) {
        { let mut logins = self.login_cancel.lock().unwrap();
          if logins.get(&provider).is_some_and(|active| Arc::ptr_eq(active, cancel)) { logins.remove(&provider); } }
        if result.is_err() || cancel.load(Ordering::Acquire) {
            if let Ok(catalog) = self.clients().catalog_snapshot(provider) { self.apply_catalog(provider, catalog); }
        }
        if cancel.load(Ordering::Acquire) || self.closing.load(Ordering::Acquire) {
            let mut view = self.view.lock().unwrap(); let state = Self::provider_ui(&mut view, provider);
            state.auth_pending = false; state.loading = false; state.status = "Вход отменён".into();
        } else { match result {
            Ok(catalog) => self.apply_catalog(provider, catalog),
            Err(error) => { let mut view = self.view.lock().unwrap(); let state = Self::provider_ui(&mut view, provider);
                state.auth_pending = false; state.loading = false; state.status = error.clone(); view.error = Some(error); }
        }}
        self.emit(app);
    }
    async fn preferred(&self, provider: ProviderId, mut catalog: Catalog) -> Result<Catalog, String> {
        let wanted = { let view = self.view.lock().unwrap(); match provider {
            ProviderId::Chatgpt => view.settings.chatgpt_model.clone(),
            ProviderId::Antigravity => view.settings.antigravity_model.clone(),
            ProviderId::Claude => view.settings.claude_model.clone(),
        }};
        if let Some(model) = wanted {
            if catalog.models.iter().any(|item| item.id == model) {
                if catalog.selected.as_deref() != Some(&model) {
                    catalog = self.clients().select_model(provider, &model).await?;
                }
            } else {
                // A saved model can disappear; the UI must require a new explicit choice.
                catalog.selected = None;
            }
        }
        Ok(catalog)
    }
    fn prepare_audio(self: &Arc<Self>, app: tauri::AppHandle) {
        let mut workers = self.audio_init_threads.lock().unwrap();
        if self.closing.load(Ordering::Acquire) || self.audio_initializing.swap(true, Ordering::AcqRel) { return; }
        workers.retain(|worker| !worker.is_finished());
        let generation = self.audio_generation.fetch_add(1, Ordering::AcqRel) + 1;
        let this = self.clone();
        let worker = std::thread::spawn(move || {
            let prepared = (|| -> Result<Transcriber, String> {
                let devices = audio::enumerate_devices()?;
                let mut view = this.view.lock().unwrap();
                let mic = devices.microphones.iter().find(|d| d.is_default).map(|d| d.id.clone());
                let system = devices.loopbacks.iter().find(|d| d.is_default).map(|d| d.id.clone());
                if view.settings.mic_device_id.is_none() { view.settings.mic_device_id = mic.clone(); }
                if view.settings.system_device_id.is_none() { view.settings.system_device_id = system.clone(); }
                view.devices = Devices {
                    microphones: devices.microphones.into_iter().map(|d| UiDevice { id: d.id, name: d.name }).collect(),
                    system: devices.loopbacks.into_iter().map(|d| UiDevice { id: d.id, name: d.name }).collect(),
                    default_microphone_id: mic, default_system_id: system,
                };
                let settings = view.settings.clone(); drop(view); this.emit(&app);
                let mut engine = Transcriber::new(Path::new(&settings.python_path), &this.worker_path, Path::new(&settings.cuda_lib_path))?;
                engine.warmup()?;
                Ok(engine)
            })();
            if generation != this.audio_generation.load(Ordering::Acquire) || this.closing.load(Ordering::Acquire) {
                this.audio_initializing.store(false, Ordering::Release); return;
            }
            match prepared {
                Ok(engine) => {
                    *this.transcriber.lock().unwrap() = Some(engine);
                    let mut view = this.view.lock().unwrap();
                    view.audio_ready = true; view.audio_status = "Whisper small готов".into();
                }
                Err(error) => {
                    let mut view = this.view.lock().unwrap();
                    view.audio_ready = false; view.audio_status = "Не удалось запустить распознавание".into(); view.error = Some(error);
                }
            }
            this.audio_initializing.store(false, Ordering::Release);
            this.emit(&app);
        });
        workers.push(worker);
    }
    fn stop_capture(&self) {
        if let Some(cancel) = self.capture_cancel.lock().unwrap().as_ref() { cancel.store(true, Ordering::Release); }
        if let Some(stop) = self.capture_stop.lock().unwrap().as_ref() { stop.request_stop(); }
    }
    fn shutdown(&self) {
        self.closing.store(true, Ordering::Release);
        self.audio_generation.fetch_add(1, Ordering::AcqRel);
        self.answer_cancel.lock().unwrap().store(true, Ordering::Release);
        for cancel in self.login_cancel.lock().unwrap().values() { cancel.store(true, Ordering::Release); }
        self.stop_capture();
        let workers = std::mem::take(&mut *self.audio_init_threads.lock().unwrap());
        for worker in workers { let _ = worker.join(); }
        self.transcriber.lock().unwrap().take();
        let clients = self.clients();
        tauri::async_runtime::block_on(clients.shutdown());
    }
}

fn provider_id(name: &str) -> Result<ProviderId, String> {
    match name { "chatgpt" => Ok(ProviderId::Chatgpt), "antigravity" => Ok(ProviderId::Antigravity), "claude" => Ok(ProviderId::Claude), _ => Err("Неизвестный сервис".into()) }
}

#[tauri::command]
fn get_state(backend: State<'_, Arc<Backend>>) -> AppState { backend.snapshot() }

#[tauri::command]
fn dismiss_error(app: tauri::AppHandle, backend: State<'_, Arc<Backend>>) {
    backend.view.lock().unwrap().error = None; backend.emit(&app);
}

#[tauri::command]
fn start_listening(app: tauri::AppHandle, backend: State<'_, Arc<Backend>>) -> Result<(), String> {
    let this = backend.inner().clone();
    let (mic, system, cancel) = {
        let mut view = this.view.lock().unwrap();
        if !view.audio_ready || view.listening || view.busy || this.close_requested.load(Ordering::Acquire) || this.closing.load(Ordering::Acquire) { return Err("Дождись готовности распознавания и завершения текущего действия".into()); }
        if view.active_room_id.is_none() { return Err("Сначала создай или открой комнату".into()); }
        let (mic, system) = view.selected_devices()?;
        let cancel = Arc::new(AtomicBool::new(false));
        *this.capture_cancel.lock().unwrap() = Some(cancel.clone());
        view.listening = true; view.audio_status = "Включаю выбранные источники звука…".into(); view.error = None;
        (mic, system, cancel)
    };
    this.emit(&app);
    std::thread::spawn(move || {
        let result = (|| -> Result<(), String> {
            let mut session = audio::start_capture(mic, system)?;
            *this.capture_stop.lock().unwrap() = Some(session.stop.clone());
            if cancel.load(Ordering::Acquire) { session.request_stop(); }
            else { this.view.lock().unwrap().audio_status = "Слушаю. Нажми «Получить ответ», когда понадобится ответ модели.".into(); this.emit(&app); }
            let mut last_level = Instant::now();
            let failure = Arc::new(Mutex::new(None::<String>));
            let (segments, pending) = std::sync::mpsc::sync_channel::<audio::AudioSegment>(6);
            let decoder_this = this.clone(); let decoder_app = app.clone();
            let decoder_stop = session.stop.clone(); let decoder_failure = failure.clone();
            let decoder = std::thread::spawn(move || {
                while let Ok(segment) = pending.recv() {
                    if decoder_this.closing.load(Ordering::Acquire) || decoder_failure.lock().unwrap().is_some() { continue; }
                    let result = decoder_this.transcriber.lock().unwrap().as_mut()
                        .ok_or_else(|| "Распознавание закрыто".to_string())
                        .and_then(|engine| engine.transcribe(&segment.samples));
                    match result {
                        Ok(text) if !text.text.trim().is_empty() => {
                            let source = match segment.source { audio::AudioSource::Microphone => "microphone", audio::AudioSource::System => "system" };
                            let text = if segment.unreliable { format!("[Возможен пропуск речи] {}", text.text) } else { text.text };
                            let mut room = decoder_this.session.lock().unwrap();
                            let saved = room.add_utterance(source, &text, segment.speech_end);
                            let saved = decoder_this.sync_saved_room(&room, saved); drop(room);
                            if let Err(error) = saved {
                                *decoder_failure.lock().unwrap() = Some(error);
                                decoder_stop.request_stop();
                            } else if saved == Ok(true) {
                                let mut view = decoder_this.view.lock().unwrap();
                                view.audio_status = format!("Последняя фраза появилась через {:.2} с", segment.speech_end.elapsed().as_secs_f64());
                                drop(view); decoder_this.emit(&decoder_app);
                            }
                        }
                        Ok(_) => {}
                        Err(error) => {
                            decoder_this.view.lock().unwrap().audio_ready = false;
                            *decoder_failure.lock().unwrap() = Some(error);
                            decoder_stop.request_stop();
                        }
                    }
                }
            });
            while let Ok(event) = session.receiver.recv() {
                match event {
                    audio::AudioEvent::Segment(segment) => {
                        if this.closing.load(Ordering::Acquire) { continue; }
                        if segment.sample_rate != audio::SAMPLE_RATE || segment.ready_at < segment.started_at {
                            *failure.lock().unwrap() = Some("Некорректный звуковой фрагмент".into()); session.stop.request_stop(); continue;
                        }
                        if segments.try_send(segment).is_err() {
                            *failure.lock().unwrap() = Some("Распознавание не успевает за звуком. Слушание остановлено.".into()); session.stop.request_stop();
                        }
                    }
                    audio::AudioEvent::Level { source, dbfs } => {
                        let level = 10.0_f32.powf(dbfs / 20.0).clamp(0.0, 1.0);
                        let mut view = this.view.lock().unwrap();
                        match source { audio::AudioSource::Microphone => view.levels.microphone = Some(level), audio::AudioSource::System => view.levels.system = Some(level) }
                        drop(view);
                        if last_level.elapsed().as_millis() >= 100 { last_level = Instant::now(); this.emit(&app); }
                    }
                    audio::AudioEvent::Error(error) => { *failure.lock().unwrap() = Some(error); session.stop.request_stop(); }
                    audio::AudioEvent::Warning(message) => { this.view.lock().unwrap().error = Some(message); this.emit(&app); }
                    audio::AudioEvent::Stopped => break,
                }
            }
            let captured = session.join();
            drop(segments);
            decoder.join().map_err(|_| "Поток распознавания неожиданно завершился")?;
            captured?;
            if let Some(error) = failure.lock().unwrap().take() { return Err(error); }
            Ok(())
        })();
        *this.capture_stop.lock().unwrap() = None;
        *this.capture_cancel.lock().unwrap() = None;
        let mut view = this.view.lock().unwrap();
        view.listening = false; view.levels = Levels::default();
        view.audio_status = "Расшифровка остановлена. Разговор сохранён.".into();
        if let Err(error) = result { view.error = Some(error); view.audio_status = "Расшифровка прервалась".into(); }
        drop(view); this.emit(&app);
        if this.close_requested.load(Ordering::Acquire) {
            if let Some(window) = app.get_webview_window("main") { let _ = window.close(); }
        }
    });
    Ok(())
}

#[tauri::command]
fn stop_listening(app: tauri::AppHandle, backend: State<'_, Arc<Backend>>) {
    backend.stop_capture();
    { let mut view = backend.view.lock().unwrap();
      if view.listening { view.audio_status = "Сохраняю последнюю фразу…".into(); } }
    backend.emit(&app);
}

#[tauri::command]
fn request_answer(app: tauri::AppHandle, backend: State<'_, Arc<Backend>>) -> Result<(), String> {
    begin_generation(app, backend.inner().clone(), false)
}

#[tauri::command]
fn summarize_room(app: tauri::AppHandle, backend: State<'_, Arc<Backend>>) -> Result<(), String> {
    begin_generation(app, backend.inner().clone(), true)
}

fn begin_generation(app: tauri::AppHandle, this: Arc<Backend>, summary: bool) -> Result<(), String> {
    let (provider, cancel, budget) = {
        let mut view = this.view.lock().unwrap();
        let provider = provider_id(&view.settings.provider)?;
        if view.busy || this.close_requested.load(Ordering::Acquire) || this.closing.load(Ordering::Acquire) { return Err("Дождись окончания запроса".into()); }
        if view.active_room_id.is_none() || view.history_total == 0 { return Err("Сначала сохрани реплику или включи расшифровку".into()); }
        if summary && view.listening { return Err("Останови расшифровку, чтобы подвести итог".into()); }
        let selected = Backend::provider_ui(&mut view, provider);
        if selected.loading || selected.auth_pending { return Err("Дождись входа в выбранный сервис или выбери другой".into()); }
        let model = selected.selected_model.clone().ok_or("Выбери доступную модель")?;
        if provider == ProviderId::Chatgpt && (!selected.connected || !selected.budget_confirmed) { return Err("Подключи ChatGPT и подтверди лимит подписки".into()); }
        if provider == ProviderId::Claude && (!selected.connected || !selected.budget_confirmed) { return Err("Подключи Claude Code и подтверди отключение дополнительного использования".into()); }
        let budget = providers::input_budget(selected.models.iter().find(|m| m.id == model).and_then(|m| m.context_window));
        let cancel = Arc::new(AtomicBool::new(false));
        *this.answer_cancel.lock().unwrap() = cancel.clone();
        view.busy = true; view.summary_busy = summary; view.error = None;
        view.context_status = if summary { "Подвожу итог комнаты…" } else { "Собираю реплики для ответа…" }.into();
        if !summary { view.answer = AnswerState { model: Some(model), streaming: true, ..Default::default() }; }
        (provider, cancel, budget)
    };
    this.emit(&app);
    tauri::async_runtime::spawn(async move {
        let stream_app = app.clone(); let stream_this = this.clone(); let stream_cancel = cancel.clone();
        let started = Instant::now();
        let on_delta: providers::DeltaSink = Arc::new(move |text| {
            if summary || stream_cancel.load(Ordering::Acquire) || stream_this.closing.load(Ordering::Acquire) { return; }
            let mut view = stream_this.view.lock().unwrap();
            if !text.is_empty() && view.answer.first_token_ms.is_none() { view.answer.first_token_ms = Some(started.elapsed().as_secs_f64() * 1000.0); }
            view.answer.text.push_str(&text); drop(view); stream_this.emit(&stream_app);
        });
        let result: Result<(), String> = async {
            let (prompt, through) = prepare_context(&this, &app, provider, &cancel, budget, summary).await?;
            check_generation(&this, &cancel)?;
            // Explicit room history is authoritative; CLI hidden history must not leak between tasks.
            this.clients().reset_context(provider).await?;
            check_generation(&this, &cancel)?;
            let answer = this.clients().answer(provider, &prompt, on_delta, cancel.clone()).await?;
            check_generation(&this, &cancel)?;
            let mut session = this.session.lock().unwrap();
            check_generation(&this, &cancel)?;
            let saved = if summary {
                session.set_summary(answer.text, through)
            } else {
                let first_ms = this.view.lock().unwrap().answer.first_token_ms;
                session.add_answer(AnswerState { text: answer.text, model: Some(answer.model), streaming: false,
                    first_token_ms: first_ms, total_ms: Some(started.elapsed().as_secs_f64() * 1000.0) })
            };
            let saved = this.sync_saved_room(&session, saved);
            if !summary { if let Some(room) = &session.room { this.view.lock().unwrap().answer = room.answer.clone(); } }
            saved
        }.await;
        if result.is_err() {
            if let Ok(catalog) = this.clients().catalog_snapshot(provider) { this.apply_catalog(provider, catalog); }
        }
        // The successful save is the commit point; a later click cannot turn it into a cancellation.
        let cancelled = result.as_ref().err().is_some_and(|error| error == "Запрос отменён");
        let previous = if result.is_err() && !summary {
            Some(this.session.lock().unwrap().room.as_ref().map(|r| r.answer.clone()).unwrap_or_default())
        } else { None };
        let mut view = this.view.lock().unwrap();
        view.answer.streaming = false;
        view.busy = false; view.summary_busy = false;
        view.context_status = if cancelled { "Запрос отменён" }
            else if result.is_err() { "Запрос не завершён" } else { "История сохранена" }.into();
        if let Err(error) = result {
            if !cancelled { view.error = Some(error); }
        }
        if let Some(previous) = previous { view.answer = previous; }
        drop(view); this.emit(&app);
    });
    Ok(())
}

fn check_generation(this: &Backend, cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Acquire) || this.closing.load(Ordering::Acquire) { Err("Запрос отменён".into()) } else { Ok(()) }
}

async fn prepare_context(this: &Arc<Backend>, app: &tauri::AppHandle, provider: ProviderId,
    cancel: &Arc<AtomicBool>, budget: usize, summary: bool) -> Result<(String, usize), String> {
    let room = this.session.lock().unwrap().room.clone().ok_or("Комната не открыта")?;
    let entry_count = room.entries.len();
    let prepared = context::prepare_with(room.entries,
        context::MemoryCheckpoint { memory: room.memory, through: room.memory_through }, budget, summary,
        |chunk| {
            let this = this.clone(); let app = app.clone(); let cancel = cancel.clone();
            async move {
                check_generation(&this, &cancel)?;
                this.view.lock().unwrap().context_status = format!("Сжимаю историю · часть {}, реплика {} из {}", chunk.id,
                    (chunk.to_entry + usize::from(chunk.to_byte > 0)).min(entry_count), entry_count);
                this.emit(&app);
                this.clients().reset_context(provider).await?;
                check_generation(&this, &cancel)?;
                this.clients().answer(provider, &chunk.prompt, Arc::new(|_| {}), cancel).await.map(|answer| answer.text)
            }
        },
        |checkpoint| {
            let mut session = this.session.lock().unwrap();
            check_generation(this, cancel)?;
            let saved = session.set_memory(checkpoint.memory, checkpoint.through);
            this.sync_saved_room(&session, saved)
        },
        || cancel.load(Ordering::Acquire) || this.closing.load(Ordering::Acquire),
    ).await?;
    Ok((prepared.prompt, prepared.through))
}

#[tauri::command]
fn cancel_answer(backend: State<'_, Arc<Backend>>) {
    if backend.view.lock().unwrap().busy { backend.answer_cancel.lock().unwrap().store(true, Ordering::Release); }
}

#[tauri::command]
async fn create_room(app: tauri::AppHandle, backend: State<'_, Arc<Backend>>) -> Result<(), String> {
    switch_room(app, backend.inner().clone(), None, None).await
}

#[tauri::command]
async fn open_room(room_id: String, entry_id: Option<String>, app: tauri::AppHandle, backend: State<'_, Arc<Backend>>) -> Result<(), String> {
    switch_room(app, backend.inner().clone(), Some(room_id), entry_id).await
}

async fn switch_room(app: tauri::AppHandle, this: Arc<Backend>, id: Option<String>, entry_id: Option<String>) -> Result<(), String> {
    this.begin_room_change()?; this.emit(&app);
    let result: Result<(), String> = async {
        let mut session = this.session.lock().unwrap();
        let changed = match id { Some(id) => session.open(&id), None => session.create() };
        this.sync_saved_room(&session, changed)?;
        let room = session.room.as_ref().unwrap();
        let matched = entry_id.as_ref().and_then(|id| room.entries.iter().position(|entry| &entry.id == id));
        { let mut view = this.view.lock().unwrap();
          view.history_limit = matched.map(|index| room.entries.len() - index).unwrap_or(200).max(200);
          view.focus_entry_id = if matched.is_some() { entry_id.clone() } else { None };
          if entry_id.is_some() && matched.is_none() { view.error = Some("Реплика из результата поиска больше не найдена. Обнови поиск.".into()); }
          view.answer = room.answer.clone();
          view.menu_request_id = 0;
          view.context_status = "История сохраняется целиком на этом компьютере".into(); }
        this.sync_room(&session);
        Ok(())
    }.await;
    this.view.lock().unwrap().busy = false;
    this.emit(&app); result
}

#[tauri::command]
fn rename_room(title: String, app: tauri::AppHandle, backend: State<'_, Arc<Backend>>) -> Result<(), String> {
    backend.begin_room_change()?;
    let result = (|| {
        let mut session = backend.session.lock().unwrap();
        session.room.as_mut().ok_or("Комната не открыта")?.rename(&title)?;
        let saved = session.save(); backend.sync_saved_room(&session, saved)
    })();
    backend.view.lock().unwrap().busy = false; backend.emit(&app); result
}

#[tauri::command]
fn pin_room(room_id: String, pinned: bool, app: tauri::AppHandle, backend: State<'_, Arc<Backend>>) -> Result<(), String> {
    backend.begin_room_change()?;
    let result = (|| {
        let mut session = backend.session.lock().unwrap();
        if session.room.as_ref().is_some_and(|r| r.id == room_id) {
            session.room.as_mut().unwrap().pinned = pinned;
            let saved = session.save(); backend.sync_saved_room(&session, saved)?;
        } else {
            let mut room = session.store.load(&room_id)?; room.pinned = pinned;
            session.store.save(&room)?;
            let (rooms, recoveries) = session.store.list_available()?;
            let mut view = backend.view.lock().unwrap(); view.rooms = rooms; view.room_recoveries = recoveries;
        }
        Ok(())
    })();
    backend.view.lock().unwrap().busy = false; backend.emit(&app); result
}

#[tauri::command]
fn add_message(text: String, app: tauri::AppHandle, backend: State<'_, Arc<Backend>>) -> Result<(), String> {
    if text.trim().is_empty() || text.len() > 1_000_000 || text.contains('\0') {
        return Err("Введи текст до 1 МБ без нулевых символов".into());
    }
    backend.begin_room_change()?;
    let result = {
        let mut session = backend.session.lock().unwrap();
        let saved = session.add_utterance("user", &text, Instant::now()).map(|_| ());
        backend.sync_saved_room(&session, saved)
    };
    backend.view.lock().unwrap().busy = false; backend.emit(&app); result
}

#[tauri::command]
fn load_more_history(app: tauri::AppHandle, backend: State<'_, Arc<Backend>>, before: Option<usize>) -> Result<(), String> {
    { let mut view = backend.view.lock().unwrap();
      if view.listening || view.busy { return Err("Останови расшифровку и дождись окончания запроса".into()); }
      let first = view.history_total.saturating_sub(view.history_limit);
      let previous_page = before.unwrap_or(first).min(first).saturating_sub(200);
      view.history_limit = (view.history_total - previous_page).max(200); }
    backend.sync_room(&backend.session.lock().unwrap()); backend.emit(&app); Ok(())
}

#[tauri::command]
fn export_room(room_id: String, backend: State<'_, Arc<Backend>>) -> Result<String, String> {
    let session = backend.session.lock().unwrap();
    if let Some(room) = session.room.as_ref().filter(|r| r.id == room_id) { return Ok(room.markdown()); }
    Ok(session.store.load(&room_id)?.markdown())
}

#[tauri::command]
fn trash_room(room_id: String, app: tauri::AppHandle, backend: State<'_, Arc<Backend>>) -> Result<(), String> {
    backend.begin_room_change()?;
    let result = (|| {
        let mut session = backend.session.lock().unwrap();
        session.trash(&room_id)?;
        { let mut view = backend.view.lock().unwrap();
          view.rooms.retain(|room| room.id != room_id); view.room_recoveries.retain(|room| room.id != room_id);
          view.deletion_notice = Some(room_id); view.menu_request_id = 0; }
        backend.refresh_room_lists(&session);
        backend.sync_room(&session);
        Ok(())
    })();
    backend.view.lock().unwrap().busy = false; backend.emit(&app); result
}

#[tauri::command]
fn restore_room(room_id: String, app: tauri::AppHandle, backend: State<'_, Arc<Backend>>) -> Result<(), String> {
    backend.begin_room_change()?;
    let result = (|| {
        let mut session = backend.session.lock().unwrap();
        session.restore(&room_id)?;
        { let mut view = backend.view.lock().unwrap();
          view.trash_rooms.retain(|room| room.id != room_id);
          if view.deletion_notice.as_deref() == Some(&room_id) { view.deletion_notice = None; } }
        backend.refresh_room_lists(&session);
        backend.sync_room(&session);
        Ok(())
    })();
    backend.view.lock().unwrap().busy = false; backend.emit(&app); result
}

#[tauri::command]
async fn search_rooms(query: String, app: tauri::AppHandle, backend: State<'_, Arc<Backend>>) -> Result<Vec<rooms::RoomSearchHit>, String> {
    let store = backend.session.lock().unwrap().store.clone();
    let (hits, recoveries) = tauri::async_runtime::spawn_blocking(move || {
        let hits = store.search(&query)?;
        let (_, recoveries) = store.list_available()?;
        Ok::<_, String>((hits, recoveries))
    }).await.map_err(|_| "Не удалось закончить поиск. Попробуй ещё раз".to_string())??;
    apply_search_recoveries(&mut backend.view.lock().unwrap(), recoveries);
    backend.emit(&app);
    Ok(hits)
}

fn apply_search_recoveries(view: &mut AppState, mut recoveries: Vec<rooms::RoomRecoveryInfo>) {
    // A search can finish after the user has moved a room to trash.
    recoveries.retain(|room| !view.trash_rooms.iter().any(|deleted| deleted.id == room.id)
        && view.deletion_notice.as_deref() != Some(room.id.as_str()));
    view.room_recoveries = recoveries;
}

#[cfg(test)]
mod search_tests {
    use super::*;
    fn issue(id: &str) -> rooms::RoomRecoveryInfo {
        rooms::RoomRecoveryInfo { id: id.into(), primary_error: Some("Повреждён файл".into()), backup: None, backup_error: None, available: false }
    }
    #[test]
    fn late_search_does_not_restore_deleted_rooms_to_the_recovery_panel() {
        let mut view = AppState::new(Settings::defaults(Path::new("C:/test")));
        let deleted = "a".repeat(32); let just_deleted = "b".repeat(32); let retained = "c".repeat(32);
        let pending_result = vec![issue(&deleted), issue(&just_deleted), issue(&retained)];
        view.trash_rooms.push(rooms::RoomTrashInfo { id: deleted.clone(), title: "Удалённая комната".into(), deleted_at: "2026-11-17T00:00:00Z".into(), entry_count: None });
        // The newest deletion must also be hidden if refreshing the trash list failed.
        view.deletion_notice = Some(just_deleted);
        apply_search_recoveries(&mut view, pending_result);
        assert_eq!(view.room_recoveries.iter().map(|room| room.id.as_str()).collect::<Vec<_>>(), vec![retained.as_str()]);
        view.trash_rooms.clear(); view.deletion_notice = None;
        apply_search_recoveries(&mut view, vec![issue(&deleted)]);
        assert_eq!(view.room_recoveries[0].id, deleted);
    }
}

#[tauri::command]
fn recover_room(room_id: String, app: tauri::AppHandle, backend: State<'_, Arc<Backend>>) -> Result<(), String> {
    backend.begin_room_change()?;
    let result: Result<(), String> = (|| {
        let mut session = backend.session.lock().unwrap();
        let recovery = session.recover(&room_id)?;
        let listing = session.store.list_available();
        let mut notice = "Комната восстановлена из резервной копии.".to_owned();
        if let Some(path) = recovery.preserved_path { notice.push_str(&format!(" Повреждённый файл сохранён: {path}.")); }
        if let Some(path) = recovery.preserved_memory_path { notice.push_str(&format!(" Несохранённый текст сохранён отдельно: {path}.")); }
        { let mut view = backend.view.lock().unwrap();
          view.room_recoveries.retain(|room| room.id != room_id);
          match listing { Ok((rooms, recoveries)) => { view.rooms = rooms; view.room_recoveries = recoveries; }, Err(error) => view.error = Some(error) }
          view.recovery_notice = Some(notice);
          view.history_limit = 200; view.answer = session.room.as_ref().map(|r| r.answer.clone()).unwrap_or_default(); }
        backend.view.lock().unwrap().focus_entry_id = None;
        backend.sync_room(&session);
        Ok(())
    })();
    backend.view.lock().unwrap().busy = false; backend.emit(&app); result
}


#[tauri::command]
fn connect_chatgpt(app: tauri::AppHandle, backend: State<'_, Arc<Backend>>) -> Result<(), String> {
    let cancel = backend.begin_login(ProviderId::Chatgpt)?; backend.emit(&app);
    let this = backend.inner().clone();
    tauri::async_runtime::spawn(async move {
        let open_error = Arc::new(Mutex::new(None::<String>));
        let event_error = open_error.clone(); let event_cancel = cancel.clone();
        let event_app = app.clone(); let event_this = this.clone();
        let emit: providers::EventSink = Arc::new(move |event| match event {
            ProviderEvent::AuthUrl { url } => {
                let result = if !url.starts_with("https://auth.openai.com/") { Err("Неверный адрес входа".to_string()) }
                    else { event_app.opener().open_url(url, None::<&str>).map_err(|error| format!("Не удалось открыть браузер: {error}")) };
                if let Err(error) = result {
                    *event_error.lock().unwrap() = Some(error.clone());
                    event_cancel.store(true, Ordering::Release); event_this.fail(&event_app, error);
                }
            }
            ProviderEvent::Authenticated { .. } => {
                let mut view = event_this.view.lock().unwrap();
                view.providers.chatgpt.connected = true; view.providers.chatgpt.status = "Ты вошёл в ChatGPT. Проверь лимит приложения.".into();
                drop(view); event_this.emit(&event_app);
            }
        });
        let result = this.clients().connect_chatgpt(emit, cancel.clone()).await
            .map_err(|error| open_error.lock().unwrap().take().unwrap_or(error));
        this.finish_login(&app, ProviderId::Chatgpt, &cancel, result);
    });
    Ok(())
}

#[tauri::command]
fn cancel_provider_login(provider: ProviderId, app: tauri::AppHandle, backend: State<'_, Arc<Backend>>) {
    let cancel = backend.login_cancel.lock().unwrap().get(&provider).cloned();
    let mut view = backend.view.lock().unwrap(); let state = Backend::provider_ui(&mut view, provider);
    cancel_login_wait(state, cancel.as_deref());
    drop(view); backend.emit(&app);
}

fn cancel_login_wait(state: &mut ProviderState, cancel: Option<&AtomicBool>) {
    if let Some(cancel) = cancel { cancel.store(true, Ordering::Release); state.status = "Отменяю ожидание входа…".into(); }
    else {
        state.auth_pending = false;
        // An external login wait does not own an in-flight catalog refresh.
        if !state.loading { state.status = "Вход отменён".into(); }
    }
}

#[tauri::command]
fn connect_provider(provider: ProviderId, app: tauri::AppHandle, backend: State<'_, Arc<Backend>>) -> Result<(), String> {
    if provider == ProviderId::Chatgpt { return connect_chatgpt(app, backend); }
    let this = backend.inner().clone();
    if provider == ProviderId::Claude {
        let cancel = this.begin_login(provider)?; this.emit(&app);
        tauri::async_runtime::spawn(async move {
            let result = match this.clients().login_claude(cancel.clone()).await {
                Ok(catalog) => this.preferred(provider, catalog).await,
                Err(error) => Err(error),
            };
            this.finish_login(&app, provider, &cancel, result);
        });
    } else {
        this.begin_provider(provider)?; this.emit(&app);
        tauri::async_runtime::spawn(async move {
            if this.closing.load(Ordering::Acquire) || this.close_requested.load(Ordering::Acquire) { return; }
            #[cfg(windows)]
            let executable = std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
                .map(|root| root.join("Programs/antigravity/Antigravity.exe"));
            #[cfg(windows)]
            let opened = match executable.filter(|path| path.is_file()) {
                Some(path) => app.opener().open_path(path.to_string_lossy().into_owned(), None::<&str>)
                    .map_err(|_| "Не удалось открыть Antigravity. Запусти официальный клиент и выполни вход.".to_owned()),
                None => Err("Установи Antigravity, войди в аккаунт и нажми «Проверить вход».".into()),
            };
            #[cfg(target_os = "linux")]
            let opened: Result<(), String> = match tokio::process::Command::new("gtk-launch").arg("antigravity").status().await {
                Ok(status) if status.success() => Ok(()),
                _ => Err("Открой Antigravity, войди в аккаунт и нажми «Проверить вход».".into()),
            };
            let mut view = this.view.lock().unwrap(); let state = Backend::provider_ui(&mut view, provider);
            state.loading = false; state.auth_pending = opened.is_ok();
            match opened {
                Ok(()) => state.status = "Войди в Antigravity, затем нажми «Проверить вход»".into(),
                Err(error) => { state.status = error.clone(); view.error = Some(error); }
            }
            drop(view); this.emit(&app);
        });
    }
    Ok(())
}

#[tauri::command]
async fn confirm_claude_subscription(app: tauri::AppHandle, backend: State<'_, Arc<Backend>>) -> Result<(), String> {
    backend.begin_provider(ProviderId::Claude)?; backend.emit(&app);
    let result = backend.clients().confirm_claude_subscription().await;
    match result {
        Ok(catalog) => match backend.preferred(ProviderId::Claude, catalog).await {
            Ok(catalog) => { backend.apply_catalog(ProviderId::Claude, catalog); backend.emit(&app); Ok(()) }
            Err(error) => { backend.provider_failed(&app, ProviderId::Claude, error.clone()); Err(error) }
        },
        Err(error) => { backend.provider_failed(&app, ProviderId::Claude, error.clone()); Err(error) }
    }
}

#[tauri::command]
fn open_provider_usage(provider: ProviderId, app: tauri::AppHandle) -> Result<(), String> {
    let url = match provider {
        ProviderId::Chatgpt => "https://chatgpt.com/settings/usage",
        ProviderId::Claude => "https://claude.ai/settings/usage",
        ProviderId::Antigravity => return Err("Лимиты Antigravity доступны в официальном клиенте".into()),
    };
    app.opener().open_url(url, None::<&str>).map_err(|_| "Не удалось открыть настройки подписки".into())
}

#[tauri::command]
async fn confirm_budget(percent: u8, app: tauri::AppHandle, backend: State<'_, Arc<Backend>>) -> Result<(), String> {
    backend.begin_provider(ProviderId::Chatgpt)?; backend.emit(&app);
    let result = backend.clients().confirm_budget(percent).await;
    match result {
        Ok(catalog) => match backend.preferred(ProviderId::Chatgpt, catalog).await {
            Ok(catalog) => { backend.apply_catalog(ProviderId::Chatgpt, catalog); backend.emit(&app); Ok(()) }
            Err(error) => { backend.provider_failed(&app, ProviderId::Chatgpt, error.clone()); Err(error) }
        },
        Err(error) => { backend.provider_failed(&app, ProviderId::Chatgpt, error.clone()); Err(error) }
    }
}

#[tauri::command]
async fn refresh_models(provider: ProviderId, app: tauri::AppHandle, backend: State<'_, Arc<Backend>>) -> Result<(), String> {
    backend.begin_provider(provider)?; backend.emit(&app);
    let result = backend.clients().refresh_models(provider).await;
    match result {
        Ok(catalog) => match backend.preferred(provider, catalog).await {
            Ok(catalog) => { backend.apply_catalog(provider, catalog); backend.emit(&app); Ok(()) }
            Err(error) => { backend.provider_failed(&app, provider, error.clone()); Err(error) }
        },
        Err(error) => { backend.provider_failed(&app, provider, error.clone()); Err(error) }
    }
}

#[tauri::command]
async fn select_model(provider: ProviderId, model_id: String, app: tauri::AppHandle, backend: State<'_, Arc<Backend>>) -> Result<(), String> {
    backend.begin_provider(provider)?; backend.emit(&app);
    let result = backend.clients().select_model(provider, &model_id).await;
    match result {
        Ok(catalog) => {
            backend.apply_catalog(provider, catalog);
            let write = backend.settings_lock.lock().unwrap();
            let mut view = backend.view.lock().unwrap();
            match provider { ProviderId::Chatgpt => view.settings.chatgpt_model = Some(model_id), ProviderId::Antigravity => view.settings.antigravity_model = Some(model_id), ProviderId::Claude => view.settings.claude_model = Some(model_id) }
            drop(view); let saved = backend.persist_unlocked(); drop(write); backend.emit(&app); saved
        }
        Err(error) => { backend.provider_failed(&app, provider, error.clone()); Err(error) }
    }
}

#[tauri::command]
async fn update_settings(mut settings: Settings, app: tauri::AppHandle, backend: State<'_, Arc<Backend>>) -> Result<(), String> {
    settings.validate()?;
    let old = {
        let mut view = backend.view.lock().unwrap();
        let capture_change = view.settings.mic_enabled != settings.mic_enabled || view.settings.system_enabled != settings.system_enabled
            || view.settings.mic_device_id != settings.mic_device_id || view.settings.system_device_id != settings.system_device_id
            || view.settings.python_path != settings.python_path || view.settings.cuda_lib_path != settings.cuda_lib_path
            || view.settings.antigravity_path != settings.antigravity_path || view.settings.claude_path != settings.claude_path;
        if (view.settings.python_path != settings.python_path || view.settings.cuda_lib_path != settings.cuda_lib_path)
            && backend.audio_initializing.load(Ordering::Acquire) { return Err("Дождись завершения подготовки распознавания".into()); }
        let busy_cli_change = (view.settings.antigravity_path != settings.antigravity_path && (view.providers.antigravity.loading || view.providers.antigravity.auth_pending))
            || (view.settings.claude_path != settings.claude_path && (view.providers.claude.loading || view.providers.claude.auth_pending));
        if view.busy || (view.listening && capture_change) || busy_cli_change || backend.close_requested.load(Ordering::Acquire) || backend.closing.load(Ordering::Acquire) {
            return Err("Останови расшифровку перед сменой устройств и дождись завершения операций".into());
        }
        // Model preferences are owned by select_model, never by an older settings dialog snapshot.
        settings.chatgpt_model = view.settings.chatgpt_model.clone();
        settings.antigravity_model = view.settings.antigravity_model.clone();
        settings.claude_model = view.settings.claude_model.clone();
        view.busy = true;
        view.settings.clone()
    };
    backend.emit(&app);
    let changed_antigravity = old.antigravity_path != settings.antigravity_path;
    let changed_claude = old.claude_path != settings.claude_path;
    let new_antigravity_path = settings.antigravity_path.clone();
    let new_claude_path = settings.claude_path.clone();
    let changed_audio = old.python_path != settings.python_path || old.cuda_lib_path != settings.cuda_lib_path
        || (!backend.view.lock().unwrap().audio_ready && !backend.audio_initializing.load(Ordering::Acquire));
    let result: Result<(), String> = async {
        {
            let _write = backend.settings_lock.lock().unwrap();
            { let mut view = backend.view.lock().unwrap();
              // These preferences have dedicated commands; an older dialog cannot overwrite them.
              settings.chatgpt_model = view.settings.chatgpt_model.clone();
              settings.antigravity_model = view.settings.antigravity_model.clone();
              settings.claude_model = view.settings.claude_model.clone();
              settings.hide_from_capture = view.settings.hide_from_capture;
              settings.window_opacity = view.settings.window_opacity;
              view.settings = settings; }
            if let Err(error) = backend.persist_unlocked() {
                let mut view = backend.view.lock().unwrap();
                let owned = (view.settings.chatgpt_model.clone(), view.settings.antigravity_model.clone(), view.settings.claude_model.clone(), view.settings.hide_from_capture, view.settings.window_opacity);
                view.settings = old;
                view.settings.chatgpt_model = owned.0; view.settings.antigravity_model = owned.1;
                view.settings.claude_model = owned.2;
                view.settings.hide_from_capture = owned.3; view.settings.window_opacity = owned.4;
                return Err(error);
            }
        }
        if changed_antigravity {
            backend.clients().configure_cli(ProviderId::Antigravity, PathBuf::from(new_antigravity_path)).await?;
            backend.view.lock().unwrap().providers.antigravity = ProviderState::default();
        }
        if changed_claude {
            backend.clients().configure_cli(ProviderId::Claude, PathBuf::from(new_claude_path)).await?;
            backend.view.lock().unwrap().providers.claude = ProviderState::default();
        }
        if changed_audio {
            { let mut view = backend.view.lock().unwrap(); view.audio_ready = false; view.audio_status = "Перезапуск Whisper…".into(); }
            backend.transcriber.lock().unwrap().take();
            backend.inner().prepare_audio(app.clone());
        }
        Ok(())
    }.await;
    backend.view.lock().unwrap().busy = false;
    backend.emit(&app);
    result?;
    if changed_antigravity { refresh_models(ProviderId::Antigravity, app.clone(), backend.clone()).await?; }
    if changed_claude { refresh_models(ProviderId::Claude, app, backend).await?; }
    Ok(())
}

#[tauri::command]
async fn update_window_appearance(hide_from_capture: bool, window_opacity: u8,
    app: tauri::AppHandle, backend: State<'_, Arc<Backend>>) -> Result<(), String> {
    if !(30..=100).contains(&window_opacity) { return Err("Непрозрачность окна должна быть от 30 до 100%".into()); }
    let this = backend.inner().clone();
    let handle = app.clone();
    let (sender, receiver) = tokio::sync::oneshot::channel();
    app.run_on_main_thread(move || {
        let result = (|| -> Result<(), String> {
            if this.closing.load(Ordering::Acquire) || this.close_requested.load(Ordering::Acquire) { return Err("Окно закрывается".into()); }
            let window = handle.get_webview_window("main").ok_or("Окно приложения не найдено")?;
            let _write = this.settings_lock.lock().unwrap();
            let old = { let view = this.view.lock().unwrap(); (view.settings.hide_from_capture, view.settings.window_opacity) };
            if let Err(error) = window_appearance::apply(&window, hide_from_capture, window_opacity) {
                this.view.lock().unwrap().window_appearance_applied = false;
                return Err(error);
            }
            { let mut view = this.view.lock().unwrap();
              view.settings.hide_from_capture = hide_from_capture; view.settings.window_opacity = window_opacity;
              view.window_appearance_applied = true; }
            if let Err(error) = this.persist_unlocked() {
                let rollback = window_appearance::apply(&window, old.0, old.1);
                let mut view = this.view.lock().unwrap();
                view.settings.hide_from_capture = old.0; view.settings.window_opacity = old.1;
                view.window_appearance_applied = rollback.is_ok();
                return Err(match rollback { Ok(()) => error, Err(rollback_error) => format!("{error}. Не удалось вернуть оформление окна: {rollback_error}") });
            }
            Ok(())
        })();
        if let Err(error) = &result { this.view.lock().unwrap().error = Some(error.clone()); }
        this.emit(&handle);
        let _ = sender.send(result);
    }).map_err(|_| "Не удалось передать настройки окна Windows".to_string())?;
    receiver.await.map_err(|_| "Окно закрылось до завершения настройки".to_string())?
}

#[tauri::command]
fn open_usage_settings(app: tauri::AppHandle) -> Result<(), String> {
    app.opener().open_url("https://chatgpt.com/settings/usage", None::<&str>).map_err(|_| "Не удалось открыть настройки ChatGPT".into())
}

fn workspace_path() -> PathBuf { PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().parent().unwrap().to_path_buf() }

fn show_main_window(app: &tauri::AppHandle) -> Result<(), String> {
    let window = app.get_webview_window("main").ok_or("Окно приложения не найдено")?;
    window.unminimize().map_err(|_| "Не удалось развернуть окно")?;
    window.show().map_err(|_| "Не удалось показать окно")?;
    window.set_focus().map_err(|_| "Не удалось активировать окно")?;
    #[cfg(windows)]
    window.set_skip_taskbar(true).map_err(|_| "Не удалось скрыть значок панели задач")?;
    Ok(())
}

fn install_hotkeys(app: &tauri::AppHandle, backend: &Arc<Backend>) {
    #[cfg(target_os = "linux")]
    if std::env::var_os("WAYLAND_DISPLAY").is_some() && std::env::var("GDK_BACKEND").as_deref() != Ok("x11") {
        backend.view.lock().unwrap().hotkey_error = Some("В Wayland открой настройки или создай комнату кнопкой в окне. Глобальные сочетания доступны в X11.".into());
        return;
    }
    let listener = backend.clone();
    let plugin = tauri_plugin_global_shortcut::Builder::new().with_handler(move |app, shortcut, event| {
        let Some(action) = hotkeys::action(shortcut, event.state()) else { return; };
        if listener.closing.load(Ordering::Acquire) || listener.close_requested.load(Ordering::Acquire) { return; }
        if let Err(error) = show_main_window(app) { listener.fail(app, error); return; }
        match action {
            hotkeys::HotkeyAction::Menu => {
                let mut view = listener.view.lock().unwrap();
                if listener.closing.load(Ordering::Acquire) || listener.close_requested.load(Ordering::Acquire) { return; }
                view.menu_request_id = listener.menu_requests.fetch_add(1, Ordering::AcqRel).wrapping_add(1);
                drop(view); listener.emit(app);
            }
            hotkeys::HotkeyAction::NewRoom => {
                let this = listener.clone(); let handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(error) = switch_room(handle.clone(), this.clone(), None, None).await { this.fail(&handle, error); }
                });
            }
        }
    }).build();
    if let Err(error) = app.plugin(plugin) {
        backend.view.lock().unwrap().hotkey_error = Some(format!("Горячие клавиши недоступны: {error}"));
        return;
    }
    let mut failures = Vec::new();
    for (shortcut, _, label) in hotkeys::bindings() {
        if let Err(error) = app.global_shortcut().register(shortcut) {
            failures.push(format!("{label}: не удалось зарегистрировать сочетание, возможно, оно занято другим приложением. {error}"));
        }
    }
    backend.view.lock().unwrap().hotkey_error = if failures.is_empty() { None } else { Some(failures.join("\n")) };
}

fn save_before_close(session: &mut RoomSession, closing: &AtomicBool) -> Result<(), String> {
    session.save()?;
    closing.store(true, Ordering::Release);
    Ok(())
}

pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .on_window_event(|window, event| {
            if window.label() != "main" { return; }
            let tauri::WindowEvent::CloseRequested { api, .. } = event else { return; };
            let backend = window.state::<Arc<Backend>>();
            let drain_capture = {
                let mut view = backend.view.lock().unwrap();
                backend.close_requested.store(true, Ordering::Release);
                if view.listening {
                    api.prevent_close();
                    view.audio_status = "Сохраняю последнюю фразу перед закрытием…".into(); true
                } else { false }
            };
            if drain_capture {
                backend.stop_capture();
                backend.emit(window.app_handle());
                return;
            }
            let result = {
                let mut session = backend.session.lock().unwrap();
                let result = save_before_close(&mut session, &backend.closing);
                backend.sync_saved_room(&session, result)
            };
            if let Err(error) = result {
                api.prevent_close();
                backend.close_requested.store(false, Ordering::Release);
                backend.fail(window.app_handle(), format!("{error}. Окно оставлено открытым, чтобы сохранить текст. Используй восстановление комнаты или экспорт Markdown."));
            }
        })
        .invoke_handler(tauri::generate_handler![get_state, dismiss_error, start_listening, stop_listening,
            request_answer, summarize_room, cancel_answer, create_room, open_room, rename_room,
            pin_room, add_message, load_more_history, export_room, search_rooms, recover_room, trash_room, restore_room,
            connect_chatgpt, connect_provider, cancel_provider_login, confirm_budget, confirm_claude_subscription, open_provider_usage,
            refresh_models, select_model, update_settings, update_window_appearance, open_usage_settings])
        .setup(|app| {
            let workspace = workspace_path();
            let config_dir = app.path().app_config_dir()?;
            fs::create_dir_all(&config_dir)?;
            let settings_file = config_dir.join("settings.json");
            let mut settings = fs::read(&settings_file).ok().and_then(|bytes| serde_json::from_slice::<Settings>(&bytes).ok())
                .filter(|s| s.validate().is_ok()).unwrap_or_else(|| Settings::defaults(&workspace));
            let resource_dir = app.path().resource_dir()?;
            #[cfg(windows)]
            let bundled_agy = resource_dir.join("vendor/agy.exe");
            #[cfg(windows)]
            if (!settings_file.exists() || !Path::new(&settings.antigravity_path).is_file()) && bundled_agy.is_file() {
                settings.antigravity_path = bundled_agy.to_string_lossy().into();
            }
            #[cfg(windows)]
            if !settings_file.exists() {
                if let Ok(bytes) = fs::read(workspace.join("identityproxy-prototype/runtime/state.json")) {
                    if let Ok(old) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                        if let Some(model) = old["selected_model"].as_str() {
                            if old["selected_provider"].as_str() == Some("Antigravity") {
                                settings.provider = "antigravity".into(); settings.antigravity_model = Some(model.into());
                            } else if old["selected_provider"].as_str() == Some("ChatGPT") { settings.chatgpt_model = Some(model.into()); }
                        }
                    }
                }
            }
            let registration_dir = config_dir.join("chatgpt");
            fs::create_dir_all(&registration_dir)?;
            #[cfg(windows)]
            for (file, key) in [("registration.json", "client_id"), ("host-id.json", "ext_agent_host_id")] {
                let destination = registration_dir.join(file);
                if !destination.exists() {
                    let source = workspace.join("identityproxy-feasibility-check/chatgpt").join(file);
                    if let Ok(bytes) = fs::read(source) {
                        if let Ok(record) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                            if let Some(id) = record.get(key).and_then(|v| v.as_str()) {
                                // Only public registration identifiers migrate; OAuth credentials never do.
                                fs::write(destination, serde_json::to_vec(&serde_json::json!({key: id}))?)?;
                            }
                        }
                    }
                }
            }
            let work_dir = config_dir.join("antigravity-work");
            let bundled_worker = resource_dir.join("resources/whisper_worker.py");
            let worker_path = if bundled_worker.is_file() { bundled_worker } else { Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/whisper_worker.py") };
            let clients = Arc::new(Providers::new(registration_dir, PathBuf::from(&settings.antigravity_path), work_dir, PathBuf::from(&settings.claude_path))?);
            let room_store = rooms::RoomStore::open(config_dir.join("rooms"))?;
            let mut initial_view = AppState::new(settings);
            match room_store.list_available() { Ok((rooms, recoveries)) => { initial_view.rooms = rooms; initial_view.room_recoveries = recoveries; }, Err(error) => initial_view.error = Some(error) }
            match room_store.trash_list() { Ok(rooms) => initial_view.trash_rooms = rooms, Err(error) => initial_view.error = Some(error) }
            let backend = Arc::new(Backend {
                view: Mutex::new(initial_view), emit_lock: Mutex::new(()), session: Mutex::new(RoomSession::new(room_store)),
                providers: clients, transcriber: Mutex::new(None), capture_stop: Mutex::new(None),
                capture_cancel: Mutex::new(None), answer_cancel: Mutex::new(Arc::new(AtomicBool::new(false))),
                login_cancel: Mutex::new(HashMap::new()),
                closing: Arc::new(AtomicBool::new(false)), close_requested: AtomicBool::new(false), menu_requests: AtomicU64::new(0), audio_generation: AtomicU64::new(0),
                audio_initializing: AtomicBool::new(false), audio_init_threads: Mutex::new(Vec::new()),
                settings_file, settings_lock: Mutex::new(()), worker_path,
            });
            app.manage(backend.clone());
            let window = app.get_webview_window("main").ok_or("Окно приложения не создано")?;
            #[cfg(windows)]
            {
                let appearance = { let view = backend.view.lock().unwrap(); (view.settings.hide_from_capture, view.settings.window_opacity) };
                window_appearance::apply(&window, appearance.0, appearance.1)?;
                window.set_skip_taskbar(true)?;
                backend.view.lock().unwrap().window_appearance_applied = true;
            }
            window.show()?;
            install_hotkeys(app.handle(), &backend);
            backend.prepare_audio(app.handle().clone());
            for provider in [ProviderId::Antigravity, ProviderId::Claude] {
                let bootstrap = backend.clone(); let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    if bootstrap.begin_provider(provider).is_err() { return; }
                    bootstrap.emit(&handle);
                    let result = match bootstrap.clients().refresh_models(provider).await {
                        Ok(catalog) => bootstrap.preferred(provider, catalog).await,
                        Err(error) => Err(error),
                    };
                    match result {
                        Ok(catalog) => bootstrap.apply_catalog(provider, catalog),
                        Err(error) => { let mut view = bootstrap.view.lock().unwrap();
                            let state = Backend::provider_ui(&mut view, provider); state.loading = false; state.connected = false; state.status = error; }
                    }
                    bootstrap.emit(&handle);
                });
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("Не удалось запустить While We Talk");
    app.run(|app, event| {
        if let tauri::RunEvent::Exit = event {
            if let Some(shortcuts) = app.try_state::<tauri_plugin_global_shortcut::GlobalShortcut<tauri::Wry>>() { let _ = shortcuts.unregister_all(); }
            app.state::<Arc<Backend>>().shutdown();
        }
    });
}

#[cfg(test)]
mod close_tests {
    use super::*;
    #[test]
    fn cancelling_login_does_not_unlock_a_running_provider_operation() {
        let mut state = ProviderState { loading: true, auth_pending: true, status: "Проверяю подключение…".into(), ..Default::default() };
        cancel_login_wait(&mut state, None);
        assert!(state.loading);
        assert!(!state.auth_pending);
        assert_eq!(state.status, "Проверяю подключение…");
        let cancel = AtomicBool::new(false);
        state.auth_pending = true;
        cancel_login_wait(&mut state, Some(&cancel));
        assert!(cancel.load(Ordering::Acquire));
        assert!(state.loading && state.auth_pending);
        state.loading = false;
        cancel_login_wait(&mut state, None);
        assert_eq!(state.status, "Вход отменён");
    }
    #[test]
    fn closing_waits_for_a_successful_save_and_retry_preserves_ready_text() {
        let directory = std::env::temp_dir().join(format!("sobesednik-close-{:032x}", rand::random::<u128>()));
        let mut session = RoomSession::new(rooms::RoomStore::open(directory.clone()).unwrap());
        session.create().unwrap();
        session.add_utterance("user", "Уже сохранённая реплика", Instant::now()).unwrap();
        let id = session.room.as_ref().unwrap().id.clone();
        let path = directory.join(format!("{id}.json"));
        let original = fs::read(&path).unwrap();
        fs::write(&path, b"broken test file").unwrap();
        assert!(session.add_utterance("user", "Готовый текст после ошибки записи", Instant::now()).is_err());
        let closing = AtomicBool::new(false);
        assert!(save_before_close(&mut session, &closing).is_err());
        assert!(!closing.load(Ordering::Acquire));
        assert_eq!(session.conversation.entries().len(), 2);
        fs::write(&path, original).unwrap();
        save_before_close(&mut session, &closing).unwrap();
        assert!(closing.load(Ordering::Acquire));
        drop(session);
        let recovered = rooms::RoomStore::open(directory.clone()).unwrap().load(&id).unwrap();
        assert_eq!(recovered.entries[1].text, "Готовый текст после ошибки записи");
        for entry in fs::read_dir(&directory).unwrap() { fs::remove_file(entry.unwrap().path()).unwrap(); }
        fs::remove_dir(directory).unwrap();
    }
}
