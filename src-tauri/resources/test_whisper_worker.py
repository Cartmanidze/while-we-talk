import io
import math
import struct
import unittest

from whisper_worker import read_audio


class ProtocolTests(unittest.TestCase):
    def test_pcm_is_read_exactly_without_eating_next_command(self):
        data = struct.pack('<ff', 0.25, -0.5)
        stream = io.BytesIO(data + b'next\n')
        self.assertEqual(read_audio(stream, 2), data)
        self.assertEqual(stream.read(), b'next\n')

    def test_truncated_payload_is_rejected(self):
        with self.assertRaisesRegex(ValueError, 'truncated'):
            read_audio(io.BytesIO(b'123'), 2)

    def test_invalid_counts_and_nonfinite_audio_are_rejected(self):
        for count in (-1, True, 320001):
            with self.assertRaises(ValueError):
                read_audio(io.BytesIO(), count)
        with self.assertRaisesRegex(ValueError, 'nonfinite'):
            read_audio(io.BytesIO(struct.pack('<f', math.nan)), 1)


if __name__ == '__main__':
    unittest.main()
