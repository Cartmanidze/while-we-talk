use std::collections::VecDeque;
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}, mpsc::{self, Receiver, SyncSender, TrySendError}};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use std::io::Write;
#[cfg(windows)]
use wasapi::{DeviceEnumerator, Direction, SampleType, StreamMode, WaveFormat};
#[cfg(target_os = "linux")]
#[path = "audio_linux.rs"]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::enumerate_devices;
#[cfg(target_os = "linux")]
use linux::capture_device;

pub const SAMPLE_RATE: u32 = 16_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AudioSource { Microphone, System }

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioDevice { pub id: String, pub name: String, pub is_default: bool }

#[derive(Debug, Clone, serde::Serialize)]
pub struct DeviceCatalog { pub microphones: Vec<AudioDevice>, pub loopbacks: Vec<AudioDevice> }

#[derive(Debug)]
pub struct AudioSegment {
    pub source: AudioSource,
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub started_at: Instant,
    pub speech_end: Instant,
    pub ready_at: Instant,
    pub unreliable: bool,
}

const MAX_FRAMES: usize = SAMPLE_RATE as usize * 15;
const PRE_FRAMES: usize = SAMPLE_RATE as usize / 5;
const SILENCE: Duration = Duration::from_millis(650);
const VOICE_THRESHOLD: f32 = 0.003;
const QUIET_BOUNDARY_THRESHOLD: f32 = 0.0001; // -80 dBFS, well below the speech gate.

struct Segmenter {
    source: AudioSource,
    pre: VecDeque<f32>,
    pre_end: Option<Instant>,
    samples: Vec<f32>,
    started: Option<Instant>,
    last_voice: Option<Instant>,
    voiced_frames: usize,
    unreliable: bool,
}
impl Segmenter {
    fn new(source: AudioSource) -> Self {
        Self { source, pre: VecDeque::with_capacity(PRE_FRAMES), pre_end: None,
               samples: Vec::new(), started: None, last_voice: None, voiced_frames: 0, unreliable: false }
    }
    fn push(&mut self, samples: &[f32], started: Instant) -> Result<Vec<AudioSegment>, String> {
        if samples.iter().any(|sample| !sample.is_finite()) { return Err("Аудиоустройство вернуло повреждённый звук".into()); }
        let mut output = Vec::new();
        let mut offset = 0;
        while offset < samples.len() {
            let now = started + frames_duration(offset);
            let end = (offset + (MAX_FRAMES - self.samples.len()).min(320)).min(samples.len());
            let block = &samples[offset..end];
            let block_end = now + frames_duration(block.len());
            // shortcut: energy defines phrase boundaries; use a streaming speech VAD for noisy rooms.
            let voiced = rms(block) >= VOICE_THRESHOLD;
            if self.samples.is_empty() && self.pre_end.is_some_and(|end| now.saturating_duration_since(end) > Duration::from_millis(100)) {
                self.pre.clear();
            }
            if self.samples.is_empty() && !voiced {
                self.pre.extend(block.iter().copied());
                while self.pre.len() > PRE_FRAMES { self.pre.pop_front(); }
                self.pre_end = Some(block_end);
                offset = end;
                continue;
            }
            if self.samples.is_empty() {
                self.started = Some(now - frames_duration(self.pre.len()));
                self.samples.extend(self.pre.drain(..));
                self.pre_end = None;
            }
            let take = block.len().min(MAX_FRAMES - self.samples.len());
            self.samples.extend_from_slice(&block[..take]);
            let ended = now + frames_duration(take);
            if voiced { self.voiced_frames += take; self.last_voice = Some(ended); }
            offset += take;
            if self.samples.len() == MAX_FRAMES || self.last_voice.is_some_and(|last| ended.saturating_duration_since(last) >= SILENCE) {
                if let Some(segment) = self.flush(ended) { output.push(segment); }
            }
        }
        Ok(output)
    }
    fn expire(&mut self, now: Instant) -> Option<AudioSegment> {
        if self.last_voice.is_some_and(|last| now.saturating_duration_since(last) >= SILENCE) { self.flush(now) } else { None }
    }
    fn discontinuity(&mut self, now: Instant) -> Option<AudioSegment> {
        self.unreliable = true;
        let previous = self.flush(now);
        self.pre.clear();
        self.pre_end = None;
        self.unreliable = true;
        previous
    }
    fn flush(&mut self, ready_at: Instant) -> Option<AudioSegment> {
        let samples = std::mem::take(&mut self.samples);
        let started = self.started.take();
        let speech_end = self.last_voice.take();
        let voiced = std::mem::take(&mut self.voiced_frames);
        if samples.len() >= SAMPLE_RATE as usize * 2 / 5 && voiced >= SAMPLE_RATE as usize / 5 {
            let unreliable = std::mem::take(&mut self.unreliable);
            Some(AudioSegment { source: self.source, samples, sample_rate: SAMPLE_RATE,
                                started_at: started.unwrap(), speech_end: speech_end.unwrap(), ready_at, unreliable })
        } else { None }
    }
    #[cfg(test)]
    fn buffered_frames(&self) -> usize { self.pre.len() + self.samples.len() }
}

fn frames_duration(frames: usize) -> Duration { Duration::from_secs_f64(frames as f64 / SAMPLE_RATE as f64) }
fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() { 0.0 } else { (samples.iter().map(|v| v * v).sum::<f32>() / samples.len() as f32).sqrt() }
}

fn is_idle_loopback_resume(source: AudioSource, empty_for: Option<Duration>, poll_gap: Duration, buffer_duration: Duration) -> bool {
    source == AudioSource::System
        && empty_for.is_some_and(|idle| idle >= Duration::from_millis(40))
        && poll_gap <= buffer_duration
}

fn is_quiet_stream_boundary(source: AudioSource, poll_gap: Duration, buffer_duration: Duration,
    has_pending_speech: bool, previous_rms: f32, packet_rms: f32) -> bool {
    let quiet_floor = previous_rms <= QUIET_BOUNDARY_THRESHOLD && packet_rms <= QUIET_BOUNDARY_THRESHOLD;
    let known_loopback_zero = source == AudioSource::System && previous_rms < VOICE_THRESHOLD && packet_rms == 0.0;
    poll_gap <= buffer_duration && !has_pending_speech && (quiet_floor || known_loopback_zero)
}

fn audio_diagnostic(value: serde_json::Value) {
    if std::env::var("SOBESEDNIK_AUDIO_DIAGNOSTICS").as_deref() != Ok("1") { return; }
    let path = std::env::temp_dir().join("sobesednik-audio-diagnostics.jsonl");
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{value}");
    }
}

#[cfg(windows)]
struct ComApartment;
#[cfg(windows)]
impl ComApartment {
    fn new() -> Result<Self, String> {
        wasapi::initialize_mta().ok().map_err(|e| format!("Не удалось открыть аудиосистему: {e}"))?;
        Ok(Self)
    }
}
#[cfg(windows)]
impl Drop for ComApartment { fn drop(&mut self) { wasapi::deinitialize(); } }

#[cfg(windows)]
pub fn enumerate_devices() -> Result<DeviceCatalog, String> {
    // COM is initialized on a dedicated thread, independent of Tauri's UI apartment.
    thread::spawn(|| {
        let _com = ComApartment::new()?;
        let enumerator = DeviceEnumerator::new().map_err(|e| e.to_string())?;
        let list = |direction| -> Result<Vec<AudioDevice>, String> {
            let default = enumerator.get_default_device(direction).and_then(|d| d.get_id()).ok();
            let collection = enumerator.get_device_collection(direction).map_err(|e| e.to_string())?;
            let mut result = Vec::new();
            for index in 0..collection.get_nbr_devices().map_err(|e| e.to_string())? {
                let device = collection.get_device_at_index(index).map_err(|e| e.to_string())?;
                let id = device.get_id().map_err(|e| e.to_string())?;
                result.push(AudioDevice { is_default: default.as_ref() == Some(&id), id,
                                          name: device.get_friendlyname().map_err(|e| e.to_string())? });
            }
            Ok(result)
        };
        Ok(DeviceCatalog { microphones: list(Direction::Capture)?, loopbacks: list(Direction::Render)? })
    }).join().map_err(|_| "Поток перечисления аудиоустройств завершился с ошибкой".to_string())?
}

#[derive(Debug)]
pub enum AudioEvent {
    Segment(AudioSegment),
    Level { source: AudioSource, dbfs: f32 },
    Warning(String),
    Error(String),
    Stopped,
}

#[derive(Clone)]
pub struct CaptureStop(Arc<AtomicBool>);
impl CaptureStop { pub fn request_stop(&self) { self.0.store(true, Ordering::Release); } }

pub struct CaptureSession {
    pub receiver: Receiver<AudioEvent>,
    pub stop: CaptureStop,
    coordinator: Option<JoinHandle<()>>,
}
impl CaptureSession {
    pub fn request_stop(&self) { self.stop.request_stop(); }
    /// Call only after consuming Stopped or after dropping the receiver.
    pub fn join(&mut self) -> Result<(), String> {
        self.coordinator.take().map_or(Ok(()), |t| t.join().map_err(|_| "Поток захвата завершился с ошибкой".into()))
    }
}
impl Drop for CaptureSession { fn drop(&mut self) { self.stop.request_stop(); } }

fn remember_error(errors: &Mutex<Option<String>>, stop: &AtomicBool, message: String) {
    let mut error = errors.lock().unwrap_or_else(|p| p.into_inner());
    if error.is_none() { *error = Some(message); }
    stop.store(true, Ordering::Release);
}

pub fn start_capture(mic_id: Option<String>, loopback_id: Option<String>) -> Result<CaptureSession, String> {
    let selected: Vec<_> = [(AudioSource::Microphone, mic_id), (AudioSource::System, loopback_id)]
        .into_iter().filter_map(|(source, id)| id.map(|id| (source, id))).collect();
    if selected.is_empty() { return Err("Выбери микрофон или системный звук".into()); }
    if selected.iter().any(|(_, id)| id.is_empty() || id.len() > 4096 || id.contains('\0')) {
        return Err("Некорректный идентификатор аудиоустройства".into());
    }
    let (tx, receiver) = mpsc::sync_channel(16);
    let (ready_tx, ready_rx) = mpsc::channel();
    let stop = Arc::new(AtomicBool::new(false));
    let errors = Arc::new(Mutex::new(None));
    let mut workers: Vec<JoinHandle<()>> = Vec::new();
    for (source, id) in selected {
        let tx = tx.clone(); let ready = ready_tx.clone(); let stopping = stop.clone(); let errors = errors.clone();
        match thread::Builder::new().name(format!("capture-{source:?}")).spawn(move || {
            let result = std::panic::catch_unwind(|| capture_device(source, &id, &tx, &ready, &stopping))
                .unwrap_or_else(|_| Err("Поток аудиоустройства аварийно завершился".into()));
            if let Err(error) = result {
                let _ = ready.send(Err(error.clone()));
                remember_error(&errors, &stopping, error);
            }
        }) {
            Ok(worker) => workers.push(worker),
            Err(error) => {
                stop.store(true, Ordering::Release);
                for worker in workers { let _ = worker.join(); }
                return Err(format!("Не удалось создать поток звука: {error}"));
            }
        }
    }
    drop(ready_tx);
    for _ in 0..workers.len() {
        let result = ready_rx.recv_timeout(Duration::from_secs(10)).unwrap_or_else(|_| Err("Аудиоустройство не ответило при запуске".into()));
        if let Err(error) = result {
            stop.store(true, Ordering::Release);
            for worker in workers { let _ = worker.join(); }
            return Err(error);
        }
    }
    let worker_stop = stop.clone();
    let coordinator = thread::spawn(move || {
        for worker in workers {
            if worker.join().is_err() { remember_error(&errors, &worker_stop, "Поток аудиоустройства аварийно завершился".into()); }
        }
        if let Some(error) = errors.lock().unwrap_or_else(|p| p.into_inner()).take() {
            let _ = tx.send(AudioEvent::Error(error));
        }
        let _ = tx.send(AudioEvent::Stopped);
    });
    Ok(CaptureSession { receiver, stop: CaptureStop(stop), coordinator: Some(coordinator) })
}

fn emit_segment(tx: &SyncSender<AudioEvent>, mut segment: AudioSegment) -> Result<(), String> {
    segment.ready_at = Instant::now();
    tx.try_send(AudioEvent::Segment(segment)).map_err(|error| match error {
        TrySendError::Full(_) => "Распознавание не успевает за звуком: очередь заполнена. Захват остановлен.".into(),
        TrySendError::Disconnected(_) => "Получатель звука закрыт".into(),
    })
}

fn recover_discontinuity(tx: &SyncSender<AudioEvent>, splitter: &mut Segmenter, source: AudioSource,
    last_warning: &mut Option<Instant>) -> Result<(), String> {
    let now = Instant::now();
    if last_warning.is_none_or(|last| now.saturating_duration_since(last) >= Duration::from_secs(1)) {
        let label = match source { AudioSource::Microphone => "Микрофон", AudioSource::System => "Системный звук" };
        tx.try_send(AudioEvent::Warning(format!("{label}: возможен пропуск речи на границе фраз. Распознавание продолжается.")))
            .map_err(|error| match error {
                TrySendError::Full(_) => "Очередь звука заполнена. Захват остановлен.".to_string(),
                TrySendError::Disconnected(_) => "Получатель звука закрыт".to_string(),
            })?;
        *last_warning = Some(now);
    }
    if let Some(previous) = splitter.discontinuity(now) { emit_segment(tx, previous)?; }
    Ok(())
}

#[cfg(windows)]
fn capture_device(source: AudioSource, id: &str, tx: &SyncSender<AudioEvent>, ready: &mpsc::Sender<Result<(), String>>, stop: &AtomicBool) -> Result<(), String> {
    let _com = ComApartment::new()?;
    let enumerator = DeviceEnumerator::new().map_err(|e| e.to_string())?;
    let device = enumerator.get_device(id).map_err(|e| format!("Аудиоустройство недоступно: {e}"))?;
    let direction = if source == AudioSource::Microphone { Direction::Capture } else { Direction::Render };
    if device.get_direction() != direction { return Err("Источник звука не соответствует выбранному устройству".into()); }
    let mut client = device.get_iaudioclient().map_err(|e| e.to_string())?;
    let format = WaveFormat::new(32, 32, SampleType::Float, SAMPLE_RATE as usize, 1, None);
    client.initialize_client(&format, Direction::Capture, StreamMode::PollingShared { autoconvert: true, buffer_duration_hns: 2_000_000 }, None)
        .map_err(|e| format!("Не удалось открыть {source:?}: {e}"))?;
    let capture = client.get_audiocaptureclient().map_err(|e| e.to_string())?;
    let buffer_frames = client.get_buffer_size().map_err(|e| e.to_string())? as usize;
    if buffer_frames == 0 || buffer_frames > SAMPLE_RATE as usize * 2 { return Err("Некорректный размер буфера аудиоустройства".into()); }
    let mut raw = vec![0u8; buffer_frames * 4];
    let mut splitter = Segmenter::new(source);
    client.start_stream().map_err(|e| e.to_string())?;
    let result = (|| {
        ready.send(Ok(())).map_err(|_| "Запуск звука отменён".to_string())?;
        let mut received_any = false;
        let mut level_time = Instant::now();
        let mut last_packet = level_time;
        let mut last_poll = level_time;
        let mut empty_since: Option<Instant> = None;
        let buffer_duration = frames_duration(buffer_frames);
        let mut level = -120.0_f32;
        let mut previous_packet_end: Option<u64> = None;
        let mut previous_rms = 0.0;
        let mut last_warning = None;
        while !stop.load(Ordering::Acquire) {
            let polled_at = Instant::now();
            let poll_gap = polled_at.saturating_duration_since(last_poll);
            last_poll = polled_at;
            let mut read_packet = false;
            while capture.get_next_packet_size().map_err(|e| format!("Аудиоустройство отключилось: {e}"))?.unwrap_or(0) > 0 {
                let (frames, info) = capture.read_from_device(&mut raw).map_err(|e| format!("Ошибка получения звука: {e}"))?;
                if frames as usize > buffer_frames { return Err("Аудиоустройство превысило размер буфера".into()); }
                let samples: Vec<f32> = raw[..frames as usize * 4].chunks_exact(4).map(|b| f32::from_le_bytes(b.try_into().unwrap())).collect();
                let packet_rms = rms(&samples);
                let idle_boundary = is_idle_loopback_resume(source,
                    empty_since.map(|empty| polled_at.saturating_duration_since(empty)), poll_gap, buffer_duration);
                let silent_boundary = is_quiet_stream_boundary(source, poll_gap, buffer_duration,
                    !splitter.samples.is_empty(), previous_rms, packet_rms);
                if !received_any || info.flags.data_discontinuity {
                    audio_diagnostic(serde_json::json!({"pid":std::process::id(),"source":source,"received_any":received_any,
                        "discontinuity":info.flags.data_discontinuity,"silent":info.flags.silent,"timestamp_error":info.flags.timestamp_error,
                        "poll_gap_ms":poll_gap.as_secs_f64()*1000.0,"empty_duration_ms":empty_since.map(|empty|polled_at.saturating_duration_since(empty).as_secs_f64()*1000.0),
                        "last_packet_gap_ms":last_packet.elapsed().as_secs_f64()*1000.0,"buffer_duration_ms":buffer_duration.as_secs_f64()*1000.0,
                        "buffer_frames":buffer_frames,"packet_frames":frames,"packet_position":info.index,"previous_packet_end":previous_packet_end,
                        "packet_timestamp":info.timestamp,"rms":packet_rms,"previous_rms":previous_rms,"active_phrase_frames":splitter.samples.len(),
                        "accepted_idle_boundary":idle_boundary,"accepted_silent_boundary":silent_boundary,
                        "last_voice_gap_ms":splitter.last_voice.map(|voice|Instant::now().saturating_duration_since(voice).as_secs_f64()*1000.0)}));
                }
                // WASAPI permits the first packet to be marked discontinuous at stream startup.
                // Loopback resumes with a discontinuity after Windows stops producing idle packets.
                // Quiet transitions are benign; other flags require a warning and marked phrase boundaries.
                if info.flags.data_discontinuity && received_any && !idle_boundary && !silent_boundary {
                    // A valid packet with a discontinuity flag is recoverable, unlike GetBuffer failure.
                    recover_discontinuity(tx, &mut splitter, source, &mut last_warning)?;
                }
                read_packet = true;
                empty_since = None;
                received_any = true;
                previous_packet_end = Some(info.index + frames as u64);
                previous_rms = packet_rms;
                last_packet = Instant::now();
                level = (20.0 * packet_rms.max(0.000001).log10()).clamp(-120.0, 0.0);
                for segment in splitter.push(&samples, last_packet - frames_duration(frames as usize))? { emit_segment(tx, segment)?; }
                if stop.load(Ordering::Acquire) { break; }
            }
            if !read_packet && empty_since.is_none() { empty_since = Some(polled_at); }
            let now = Instant::now();
            if let Some(segment) = splitter.expire(now) { emit_segment(tx, segment)?; }
            if now.saturating_duration_since(level_time) >= Duration::from_millis(100) {
                if now.saturating_duration_since(last_packet) > Duration::from_millis(150) { level = -120.0; }
                // Meter updates may be dropped; captured speech is never silently discarded.
                let _ = tx.try_send(AudioEvent::Level { source, dbfs: level });
                level_time = now;
            }
            thread::sleep(Duration::from_millis(10));
        }
        if let Some(segment) = splitter.flush(Instant::now()) { emit_segment(tx, segment)?; }
        Ok(())
    })();
    let stopped = client.stop_stream().map_err(|e| format!("Ошибка остановки аудиоустройства: {e}"));
    result.and(stopped)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn feed(segmenter: &mut Segmenter, value: f32, seconds: f64, origin: Instant, output: &mut Vec<AudioSegment>) {
        for block in 0..(seconds * 50.0) as u32 {
            output.extend(segmenter.push(&vec![value; 320], origin + Duration::from_millis(block as u64 * 20)).unwrap());
        }
    }
    #[test]
    fn silence_never_produces_a_phrase_and_memory_is_bounded() {
        let mut s = Segmenter::new(AudioSource::System);
        let mut out = vec![];
        feed(&mut s, 0.0, 30.0, Instant::now(), &mut out);
        assert!(out.is_empty());
        assert!(s.buffered_frames() <= 3200);
    }
    #[test]
    fn pause_splits_phrases_and_preserves_preroll() {
        let mut s = Segmenter::new(AudioSource::Microphone);
        let t = Instant::now(); let mut out = vec![];
        feed(&mut s, 0.0, 0.4, t, &mut out);
        feed(&mut s, 0.1, 0.6, t + Duration::from_millis(400), &mut out);
        feed(&mut s, 0.0, 0.8, t + Duration::from_secs(1), &mut out);
        feed(&mut s, 0.2, 0.6, t + Duration::from_millis(1800), &mut out);
        feed(&mut s, 0.0, 0.8, t + Duration::from_millis(2400), &mut out);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].started_at, t + Duration::from_millis(200));
        assert_eq!(out[0].speech_end, t + Duration::from_secs(1));
        assert_eq!(out[0].source, AudioSource::Microphone);
    }
    #[test]
    fn idle_loopback_finishes_without_new_silence_packets() {
        let mut s = Segmenter::new(AudioSource::System);
        let t = Instant::now(); let mut out = vec![];
        feed(&mut s, 0.2, 0.6, t, &mut out);
        assert!(s.expire(t + Duration::from_millis(1240)).is_none());
        let result = s.expire(t + Duration::from_millis(1250));
        assert!(result.is_some());
        assert_eq!(result.unwrap().samples.len(), 9600);
    }
    #[test]
    fn long_speech_is_bounded_without_dropped_samples() {
        let mut s = Segmenter::new(AudioSource::System);
        let t = Instant::now(); let mut out = vec![];
        feed(&mut s, 0.1, 31.0, t, &mut out);
        if let Some(last) = s.expire(t + Duration::from_secs(32)) { out.push(last); }
        assert_eq!(out.iter().map(|s| s.samples.len()).sum::<usize>(), 31 * SAMPLE_RATE as usize);
        assert!(out.iter().all(|s| s.samples.len() <= 15 * SAMPLE_RATE as usize));
    }
    #[test]
    fn rejects_nonfinite_samples() {
        let mut s = Segmenter::new(AudioSource::System);
        assert!(s.push(&[f32::NAN], Instant::now()).is_err());
    }
    #[test]
    fn overflowing_phrase_queue_is_an_explicit_error() {
        let (tx, _rx) = mpsc::sync_channel(1);
        tx.send(AudioEvent::Level { source: AudioSource::System, dbfs: -20.0 }).unwrap();
        let now = Instant::now();
        let segment = AudioSegment { source: AudioSource::System, samples: vec![0.1; 6400], sample_rate: SAMPLE_RATE,
                                     started_at: now, speech_end: now, ready_at: now, unreliable: false };
        assert!(emit_segment(&tx, segment).unwrap_err().contains("очередь заполнена"));
    }
    #[test]
    fn loopback_resume_after_observed_idle_is_not_capture_overflow() {
        assert!(is_idle_loopback_resume(AudioSource::System, Some(Duration::from_millis(2500)), Duration::from_millis(10), Duration::from_millis(200)));
        assert!(!is_idle_loopback_resume(AudioSource::Microphone, Some(Duration::from_millis(2500)), Duration::from_millis(10), Duration::from_millis(200)));
        assert!(!is_idle_loopback_resume(AudioSource::System, None, Duration::from_millis(10), Duration::from_millis(200)));
        assert!(!is_idle_loopback_resume(AudioSource::System, Some(Duration::from_millis(2500)), Duration::from_millis(250), Duration::from_millis(200)));
    }
    #[test]
    fn zero_loopback_boundary_between_quiet_packets_is_not_lost_speech() {
        let poll = Duration::from_micros(10407); let buffer = Duration::from_millis(200);
        assert!(is_quiet_stream_boundary(AudioSource::System, poll, buffer, false, 0.00024, 0.0));
        assert!(is_quiet_stream_boundary(AudioSource::Microphone, poll, buffer, false, 0.0, 0.0));
        assert!(!is_quiet_stream_boundary(AudioSource::System, Duration::from_millis(250), buffer, false, 0.0, 0.0));
        assert!(!is_quiet_stream_boundary(AudioSource::System, poll, buffer, true, 0.0, 0.0));
        assert!(!is_quiet_stream_boundary(AudioSource::System, poll, buffer, false, 0.1, 0.0));
        assert!(!is_quiet_stream_boundary(AudioSource::System, poll, buffer, false, 0.0, 0.1));
    }
    #[test]
    fn microphone_noise_floor_transition_does_not_stop_after_a_completed_phrase() {
        let poll = Duration::from_micros(10548);
        let buffer = Duration::from_millis(200);
        assert!(is_quiet_stream_boundary(AudioSource::Microphone, poll, buffer, false, 0.00003640696, 0.000039173268));
        assert!(!is_quiet_stream_boundary(AudioSource::Microphone, poll, buffer, true, 0.00003640696, 0.000039173268));
        assert!(!is_quiet_stream_boundary(AudioSource::Microphone, Duration::from_millis(250), buffer, false, 0.00003640696, 0.000039173268));
        assert!(!is_quiet_stream_boundary(AudioSource::Microphone, poll, buffer, false, 0.00011, 0.000039173268));
        assert!(!is_quiet_stream_boundary(AudioSource::Microphone, poll, buffer, false, 0.00003640696, 0.00011));
    }
    #[test]
    fn discontinuity_marks_adjacent_phrases_but_capture_can_continue() {
        let mut s = Segmenter::new(AudioSource::Microphone);
        let t = Instant::now(); let mut out = vec![];
        feed(&mut s, 0.1, 0.6, t, &mut out);
        let previous = s.discontinuity(t + Duration::from_millis(600));
        assert!(previous.is_some());
        assert!(previous.unwrap().unreliable);
        feed(&mut s, 0.2, 0.6, t + Duration::from_secs(1), &mut out);
        feed(&mut s, 0.0, 0.8, t + Duration::from_millis(1600), &mut out);
        feed(&mut s, 0.3, 0.6, t + Duration::from_secs(3), &mut out);
        feed(&mut s, 0.0, 0.8, t + Duration::from_millis(3600), &mut out);
        assert_eq!(out.len(), 2);
        assert!(out[0].unreliable);
        assert!(!out[1].unreliable);
    }
    #[test]
    fn recoverable_stream_flag_produces_warning_instead_of_error() {
        let (tx, rx) = mpsc::sync_channel(4);
        let mut s = Segmenter::new(AudioSource::System);
        let mut last_warning = None;
        assert!(recover_discontinuity(&tx, &mut s, AudioSource::System, &mut last_warning).is_ok());
        assert!(matches!(rx.try_recv(), Ok(AudioEvent::Warning(_))));
        let mut out = vec![]; let t = Instant::now();
        feed(&mut s, 0.2, 0.6, t, &mut out);
        feed(&mut s, 0.0, 0.8, t + Duration::from_millis(600), &mut out);
        assert_eq!(out.len(), 1);
        assert!(out[0].unreliable);
    }
    #[test]
    fn warning_coalesces_but_later_phrase_still_keeps_gap_marker() {
        let (tx, rx) = mpsc::sync_channel(4);
        let mut s = Segmenter::new(AudioSource::Microphone);
        let mut last_warning = None;
        recover_discontinuity(&tx, &mut s, AudioSource::Microphone, &mut last_warning).unwrap();
        recover_discontinuity(&tx, &mut s, AudioSource::Microphone, &mut last_warning).unwrap();
        assert_eq!(rx.try_iter().filter(|event|matches!(event,AudioEvent::Warning(_))).count(), 1);
        let mut out = vec![]; let t = Instant::now();
        feed(&mut s, 0.2, 0.6, t, &mut out);
        feed(&mut s, 0.0, 0.8, t + Duration::from_millis(600), &mut out);
        assert_eq!(out.len(), 1);
        assert!(out[0].unreliable);
    }
    #[test]
    fn tiny_noise_after_gap_does_not_consume_the_next_phrase_marker() {
        let mut s = Segmenter::new(AudioSource::System);
        let t = Instant::now(); let mut out = vec![];
        assert!(s.discontinuity(t).is_none());
        feed(&mut s, 0.2, 0.1, t, &mut out);
        feed(&mut s, 0.0, 0.8, t + Duration::from_millis(100), &mut out);
        assert!(out.is_empty());
        feed(&mut s, 0.2, 0.6, t + Duration::from_secs(2), &mut out);
        feed(&mut s, 0.0, 0.8, t + Duration::from_millis(2600), &mut out);
        assert_eq!(out.len(), 1);
        assert!(out[0].unreliable);
    }
}
