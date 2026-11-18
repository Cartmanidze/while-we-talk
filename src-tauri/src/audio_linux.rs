use super::*;
use std::collections::HashSet;
use std::io::Read;
use std::os::fd::AsRawFd;
use std::process::{Command, Stdio};

fn pactl(args: &[&str]) -> Result<String, String> {
    let out = Command::new("pactl")
        .args(args)
        .env("LC_ALL", "C")
        .output()
        .map_err(|_| "Не найден pactl. Установи пакет pulseaudio-utils.")?;
    if !out.status.success() {
        return Err(
            "Не удалось подключиться к звуку. Проверь, работает ли PulseAudio или pipewire-pulse."
                .into(),
        );
    }
    String::from_utf8(out.stdout).map_err(|_| "Не удалось прочитать список устройств".into())
}

fn catalog(
    sources: &str,
    sinks: &str,
    default_source: &str,
    default_sink: &str,
) -> Result<DeviceCatalog, String> {
    #[derive(serde::Deserialize)]
    struct Device {
        name: String,
        description: String,
        monitor_source: Option<String>,
    }
    let parse = |s: &str| {
        serde_json::from_str::<Vec<Device>>(s)
            .map_err(|_| "Не удалось прочитать устройства PulseAudio")
    };
    let mut monitors = HashSet::new();
    let mut loopbacks = Vec::new();
    for sink in parse(sinks)? {
        if let Some(id) = sink.monitor_source.filter(|s| !s.is_empty()) {
            monitors.insert(id.clone());
            loopbacks.push(AudioDevice {
                id,
                name: sink.description,
                is_default: sink.name == default_sink,
            });
        }
    }
    let microphones = parse(sources)?
        .into_iter()
        .filter(|s| !monitors.contains(&s.name))
        .map(|s| AudioDevice {
            is_default: s.name == default_source,
            id: s.name,
            name: s.description,
        })
        .collect();
    Ok(DeviceCatalog {
        microphones,
        loopbacks,
    })
}

pub fn enumerate_devices() -> Result<DeviceCatalog, String> {
    catalog(
        &pactl(&["--format=json", "list", "sources"])?,
        &pactl(&["--format=json", "list", "sinks"])?,
        pactl(&["get-default-source"])?.trim(),
        pactl(&["get-default-sink"])?.trim(),
    )
}

pub(super) fn capture_device(
    source: AudioSource,
    id: &str,
    tx: &SyncSender<AudioEvent>,
    ready: &mpsc::Sender<Result<(), String>>,
    stop: &AtomicBool,
) -> Result<(), String> {
    let mut child = Command::new("parec")
        .args([
            "--raw",
            "--format=float32le",
            "--rate=16000",
            "--channels=1",
            "--latency-msec=40",
            "--client-name=While We Talk",
        ])
        .arg(format!("--device={id}"))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "Не удалось запустить parec. Установи пакет pulseaudio-utils.")?;
    let result = (|| {
        let mut pipe = child
            .stdout
            .take()
            .ok_or("Не удалось открыть звуковой поток")?;
        let mut fd = libc::pollfd {
            fd: pipe.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let mut buf = [0u8; 2560 + 3];
        let mut carry = 0;
        let mut splitter = Segmenter::new(source);
        let mut last_level = Instant::now();
        let mut opened = false;
        let pid = child.id().to_string();
        let mut last_probe = Instant::now();
        while !stop.load(Ordering::Acquire) {
            // A suspended source may not write any bytes. Poll so Stop never waits for sound.
            let polled = unsafe { libc::poll(&mut fd, 1, 50) };
            if polled < 0 {
                let err = std::io::Error::last_os_error();
                if err.kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(format!("Ошибка чтения звука: {err}"));
            }
            let now = Instant::now();
            let level = if polled == 0 {
                // PipeWire can suspend a monitor until something plays. An established
                // recording stream is ready even if its first PCM packet hasn't arrived.
                if !opened && last_probe.elapsed() >= Duration::from_millis(500) {
                    let outputs = pactl(&["--format=json", "list", "source-outputs"])?;
                    let streams: Vec<serde_json::Value> = serde_json::from_str(&outputs)
                        .map_err(|_| "Не удалось проверить подключение устройства")?;
                    if streams.iter().any(|s| {
                        s.pointer("/properties/application.process.id")
                            .and_then(|v| v.as_str())
                            == Some(pid.as_str())
                    }) {
                        ready.send(Ok(())).map_err(|_| "Запуск отменён")?;
                        opened = true;
                    }
                    last_probe = now;
                }
                if let Some(segment) = splitter.expire(now) {
                    emit_segment(tx, segment)?;
                }
                -120.0
            } else {
                let n = pipe
                    .read(&mut buf[carry..])
                    .map_err(|_| "Не удалось прочитать звук из PulseAudio")?;
                if n == 0 {
                    return Err("Прервалась запись звука. Проверь выбранное устройство.".into());
                }
                let total = carry + n;
                let end = total / 4 * 4;
                let samples: Vec<_> = buf[..end]
                    .chunks_exact(4)
                    .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
                    .collect();
                buf.copy_within(end..total, 0);
                carry = total - end;
                if samples.is_empty() {
                    continue;
                }
                if !opened {
                    ready.send(Ok(())).map_err(|_| "Запуск отменён")?;
                    opened = true;
                }
                for segment in splitter.push(&samples, now - frames_duration(samples.len()))? {
                    emit_segment(tx, segment)?;
                }
                (20.0 * rms(&samples).max(0.000001).log10()).clamp(-120.0, 0.0)
            };
            if last_level.elapsed() >= Duration::from_millis(100) {
                let _ = tx.try_send(AudioEvent::Level {
                    source,
                    dbfs: level,
                });
                last_level = now;
            }
        }
        if let Some(segment) = splitter.flush(Instant::now()) {
            emit_segment(tx, segment)?;
        }
        Ok(())
    })();
    let _ = child.kill();
    let _ = child.wait();
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn system_audio_uses_the_sink_monitor_even_when_its_name_is_custom() {
        let devices = catalog(
            r#"[{"name":"mic","description":"Mic"},{"name":"custom","description":"Monitor"}]"#,
            r#"[{"name":"out","description":"Speakers","monitor_source":"custom"}]"#,
            "mic",
            "out",
        )
        .unwrap();
        assert_eq!(devices.microphones.len(), 1);
        assert!(devices.microphones[0].is_default);
        assert_eq!(devices.loopbacks[0].id, "custom");
        assert!(devices.loopbacks[0].is_default);
    }

    #[test]
    #[ignore = "Needs an isolated PulseAudio server with two test sinks and a speech WAV fixture"]
    fn virtual_microphone_and_monitor_both_produce_speech() {
        use std::path::Path;
        let mic_wav = std::env::var("WWT_TEST_MIC_WAV").unwrap();
        let system_wav = std::env::var("WWT_TEST_SYSTEM_WAV").unwrap();
        let python = std::env::var("WWT_TEST_PYTHON").unwrap();
        let devices = enumerate_devices().unwrap();
        assert!(devices.microphones.iter().any(|d| d.id == "wwt_test_mic"));
        assert!(devices
            .loopbacks
            .iter()
            .any(|d| d.id == "wwt_test_output.monitor"));
        let mut whisper = crate::transcription::Transcriber::new(
            Path::new(&python),
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/whisper_worker.py"),
            Path::new(""),
        )
        .unwrap();
        whisper.warmup().unwrap();
        let mut session = start_capture(
            Some("wwt_test_mic".into()),
            Some("wwt_test_output.monitor".into()),
        )
        .unwrap();
        let playback = thread::spawn(move || {
            for (sink, wav) in [("wwt_test_input", mic_wav), ("wwt_test_output", system_wav)] {
                assert!(Command::new("paplay")
                    .arg(format!("--device={sink}"))
                    .arg(wav)
                    .status()
                    .unwrap()
                    .success());
            }
        });
        let deadline = Instant::now() + Duration::from_secs(24);
        let mut captured = [None, None];
        while captured.iter().any(Option::is_none) {
            match session
                .receiver
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .unwrap()
            {
                AudioEvent::Segment(segment) => {
                    let idx = usize::from(segment.source == AudioSource::System);
                    captured[idx].get_or_insert(segment);
                }
                AudioEvent::Error(e) => panic!("{e}"),
                AudioEvent::Stopped => panic!("Capture stopped before both phrases arrived"),
                _ => {}
            }
        }
        session.request_stop();
        while !matches!(
            session
                .receiver
                .recv_timeout(Duration::from_secs(3))
                .unwrap(),
            AudioEvent::Stopped
        ) {}
        session.join().unwrap();
        playback.join().unwrap();
        for segment in captured.into_iter().flatten() {
            let result = whisper.transcribe(&segment.samples).unwrap();
            println!(
                "{:?}: {} ({:.2}s)",
                segment.source, result.text, result.seconds
            );
            // This checks routing and recognition; synthetic voices can distort individual words.
            let expected = match segment.source {
                AudioSource::Microphone => ["сегодня", "провер", "работ", "микрофон"],
                AudioSource::System => ["хорош", "погод", "светит", "солнц"],
            };
            let text = result.text.to_lowercase();
            assert!(
                expected.iter().filter(|word| text.contains(**word)).count() >= 2,
                "Wrong phrase for {:?}",
                segment.source
            );
        }
    }
}
