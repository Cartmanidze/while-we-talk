use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use serde_json::{json, Value};
#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WarmupResult { pub load_seconds: f64, pub warmup_seconds: f64 }

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Transcription { pub text: String, pub seconds: f64 }

fn encode_audio(samples: &[f32]) -> Result<Vec<u8>, String> {
    if samples.len() > 320000 || samples.iter().any(|value| !value.is_finite()) {
        return Err("Некорректный или слишком длинный фрагмент звука".into());
    }
    let mut bytes = Vec::with_capacity(samples.len() * 4);
    for value in samples { bytes.extend_from_slice(&value.to_le_bytes()); }
    Ok(bytes)
}

pub struct Transcriber {
    child: Child,
    input: BufWriter<ChildStdin>,
    output: Receiver<Result<Value, String>>,
    reader: Option<JoinHandle<()>>,
    healthy: bool,
}

impl Transcriber {
    pub fn new(python_path: &Path, worker_path: &Path, cuda_lib_dir: &Path) -> Result<Self, String> {
        if !python_path.is_file() || !worker_path.is_file() || !cuda_lib_dir.is_dir() {
            return Err("Проверь пути к Python, локальному Whisper и библиотекам CUDA".into());
        }
        let mut command = Command::new(python_path);
        command.args(["-X", "utf8", "-u"]).arg(worker_path).arg("--cuda-lib-dir").arg(cuda_lib_dir)
            .env("PYTHONUTF8", "1").stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
        #[cfg(windows)]
        command.creation_flags(0x08000000);
        let mut child = command.spawn().map_err(|e| format!("Не удалось запустить локальный Whisper: {e}"))?;
        let input = BufWriter::new(child.stdin.take().ok_or("Нет входного канала Whisper")?);
        let stdout = child.stdout.take().ok_or("Нет выходного канала Whisper")?;
        let (tx, output) = mpsc::sync_channel(2);
        let reader = thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut line = Vec::new();
                let read = (&mut reader).take(65_537).read_until(b'\n', &mut line);
                let response = match read {
                    Ok(0) => Err("Локальный процесс Whisper завершился".to_string()),
                    Ok(_) if line.len() <= 65_536 && line.ends_with(b"\n") =>
                        serde_json::from_slice(&line).map_err(|_| "Whisper вернул неверный формат ответа".to_string()),
                    Ok(_) => Err("Whisper превысил размер сообщения".to_string()),
                    Err(_) => Err("Не удалось прочитать ответ Whisper".to_string()),
                };
                let failed = response.is_err();
                if tx.try_send(response).is_err() || failed { break; }
            }
        });
        let mut worker = Self { child, input, output, reader: Some(reader), healthy: true };
        worker.receive("ready", Duration::from_secs(15))?;
        Ok(worker)
    }

    fn receive(&mut self, expected: &str, timeout: Duration) -> Result<Value, String> {
        let value = self.output.recv_timeout(timeout).map_err(|_| "Истекло время ожидания локального Whisper".to_string())
            .and_then(|value| value);
        match value {
            Ok(value) if value.get("type").and_then(Value::as_str) == Some(expected) => Ok(value),
            Ok(value) => {
                self.healthy = false;
                let _ = self.child.kill();
                let message = value.get("message").and_then(Value::as_str).unwrap_or("Неожиданный ответ локального Whisper");
                Err(format!("Локальный Whisper: {}", message.chars().take(600).collect::<String>()))
            }
            Err(error) => { self.healthy = false; let _ = self.child.kill(); Err(error) }
        }
    }

    fn request(&mut self, header: Value, data: &[u8], expected: &str, timeout: Duration) -> Result<Value, String> {
        if !self.healthy { return Err("Перезапусти приложение, чтобы снова включить распознавание".into()); }
        let sent = (|| -> std::io::Result<()> {
            writeln!(self.input, "{header}")?;
            self.input.write_all(data)?;
            self.input.flush()
        })();
        if sent.is_err() {
            self.healthy = false;
            let _ = self.child.kill();
            return Err("Не удалось передать звук в локальный Whisper".into());
        }
        self.receive(expected, timeout)
    }

    pub fn warmup(&mut self) -> Result<WarmupResult, String> {
        let value = self.request(json!({"command":"warmup"}), &[], "warmup", Duration::from_secs(90))?;
        let result: WarmupResult = serde_json::from_value(value).map_err(|_| "Whisper вернул неверные параметры прогрева")?;
        if !result.load_seconds.is_finite() || !result.warmup_seconds.is_finite() || result.load_seconds < 0.0 || result.warmup_seconds < 0.0 {
            return Err("Whisper вернул некорректное время прогрева".into());
        }
        Ok(result)
    }

    pub fn transcribe(&mut self, samples: &[f32]) -> Result<Transcription, String> {
        let bytes = encode_audio(samples)?;
        let value = self.request(json!({"command":"transcribe","samples":samples.len()}), &bytes, "result", Duration::from_secs(60))?;
        let result: Transcription = serde_json::from_value(value).map_err(|_| "Whisper вернул неверную расшифровку")?;
        if result.text.len() > 50000 || !result.seconds.is_finite() || result.seconds < 0.0 {
            return Err("Whisper вернул некорректную расшифровку".into());
        }
        Ok(result)
    }
}

impl Drop for Transcriber {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = writeln!(self.input, "{}", json!({"command":"shutdown"}));
            let _ = self.input.flush();
            let deadline = Instant::now() + Duration::from_secs(2);
            while self.child.try_wait().ok().flatten().is_none() && Instant::now() < deadline { thread::sleep(Duration::from_millis(20)); }
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() { let _ = reader.join(); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn encodes_binary_pcm_without_json_expansion() {
        assert_eq!(encode_audio(&[0.25, -0.5]).unwrap(), [0.25_f32.to_le_bytes(), (-0.5_f32).to_le_bytes()].concat());
    }
    #[test]
    fn rejects_nonfinite_and_oversized_audio() {
        assert!(encode_audio(&[f32::INFINITY]).is_err());
        assert!(encode_audio(&vec![0.1; 320001]).is_err());
    }
}
