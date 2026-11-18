#!/bin/sh
# Run from the repository root. The private server has no hardware inputs or outputs.
set -eu
umask 077
tmp=$(mktemp -d)
export PULSE_SERVER="unix:$tmp/pulse"
pulseaudio -n --daemonize=no --use-pid-file=no --exit-idle-time=-1 \
  --load="module-native-protocol-unix socket=$tmp/pulse" >"$tmp/pulse.log" 2>&1 &
server=$!
trap 'kill "$server" 2>/dev/null || :; wait "$server" 2>/dev/null || :; rm -f "$tmp/mic.wav" "$tmp/system.wav" "$tmp/pulse.log" "$tmp/pulse"; rmdir "$tmp"' EXIT
for i in $(seq 1 50); do
  if pactl info >/dev/null 2>&1; then break; fi
  sleep 0.1
done
pactl load-module module-null-sink sink_name=wwt_test_input >/dev/null
pactl load-module module-null-sink sink_name=wwt_test_output >/dev/null
pactl load-module module-remap-source master=wwt_test_input.monitor source_name=wwt_test_mic >/dev/null
export WWT_TEST_MIC_WAV="$tmp/mic.wav"
export WWT_TEST_SYSTEM_WAV="$tmp/system.wav"
export WWT_TEST_PYTHON="${WWT_TEST_PYTHON:-$HOME/.local/share/while-we-talk/whisper/bin/python}"
espeak-ng -v ru -s 145 -w "$WWT_TEST_MIC_WAV" 'Сегодня мы проверяем работу микрофона.'
espeak-ng -v ru -s 145 -w "$WWT_TEST_SYSTEM_WAV" 'Сегодня хорошая погода и светит солнце.'
cargo test --locked --release --features tauri/custom-protocol --manifest-path src-tauri/Cargo.toml \
  --lib audio::linux::tests::virtual_microphone_and_monitor_both_produce_speech -- --ignored --exact --nocapture
