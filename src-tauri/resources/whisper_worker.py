"""GPU inference only. Rust owns devices, phrase boundaries, context and providers."""
import argparse
import json
import math
import os
from pathlib import Path
import struct
import sys
import time


def read_audio(stream, count):
    if type(count) is not int or not 0 <= count <= 320000:
        raise ValueError('invalid_sample_count')
    remaining = count * 4
    chunks = []
    while remaining:
        chunk = stream.read(remaining)
        if not chunk:
            raise ValueError('truncated_audio')
        chunks.append(chunk)
        remaining -= len(chunk)
    data = b''.join(chunks)
    if any(not math.isfinite(value[0]) for value in struct.iter_unpack('<f', data)):
        raise ValueError('nonfinite_audio')
    return data


class WhisperInference:
    def __init__(self, cuda_lib_dir):
        self.model = None
        self.dll_handles = []
        library_root = Path(cuda_lib_dir) / 'nvidia'
        for name in ('cublas', 'cudnn'):
            directory = library_root / name / 'bin'
            if directory.is_dir():
                os.environ['PATH'] = str(directory) + os.pathsep + os.environ.get('PATH', '')
                self.dll_handles.append(os.add_dll_directory(str(directory)))
        os.environ['HF_HUB_OFFLINE'] = '1'
        os.environ['HF_HUB_DISABLE_TELEMETRY'] = '1'

    def warmup(self):
        if self.model is not None:
            return {'type': 'warmup', 'load_seconds': 0.0, 'warmup_seconds': 0.0}
        started = time.perf_counter()
        import numpy as np
        from faster_whisper import WhisperModel
        model = WhisperModel('small', device='cuda', compute_type='float16', cpu_threads=8, local_files_only=True)
        loaded = time.perf_counter()
        segments, _ = model.transcribe(np.zeros(16000, dtype=np.float32), language='ru', beam_size=1,
                                       vad_filter=False, condition_on_previous_text=False)
        list(segments)
        self.model = model
        return {'type': 'warmup', 'load_seconds': loaded - started, 'warmup_seconds': time.perf_counter() - loaded}

    def transcribe(self, data):
        self.warmup()
        import numpy as np
        started = time.perf_counter()
        audio = np.frombuffer(data, dtype='<f4')
        if len(audio) < 6400 or float(np.sqrt(np.mean(audio ** 2))) < 0.002:
            text = ''
        else:
            segments, _ = self.model.transcribe(audio, language='ru', beam_size=1,
                                                vad_filter=True, condition_on_previous_text=False)
            text = ' '.join(segment.text.strip() for segment in segments).strip()
        return {'type': 'result', 'text': text, 'seconds': time.perf_counter() - started}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--cuda-lib-dir', required=True)
    options = parser.parse_args()
    worker = WhisperInference(options.cuda_lib_dir)

    def emit(value):
        print(json.dumps(value, ensure_ascii=False, allow_nan=False), flush=True)

    emit({'type': 'ready', 'engine': 'faster-whisper', 'device': 'cuda', 'model': 'small'})
    while True:
        header = sys.stdin.buffer.readline(4097)
        if not header:
            return
        try:
            if len(header) > 4096 or not header.endswith(b'\n'):
                raise ValueError('invalid_header')
            request = json.loads(header)
            command = request.get('command')
            if command == 'shutdown':
                return
            if command == 'warmup':
                emit(worker.warmup())
            elif command == 'transcribe':
                emit(worker.transcribe(read_audio(sys.stdin.buffer, request.get('samples'))))
            else:
                raise ValueError('unknown_command')
        except Exception as error:
            emit({'type': 'error', 'message': str(error)[:600]})
            # Framing or inference failure makes reuse unsafe; the Rust host restarts explicitly.
            return


if __name__ == '__main__':
    main()
