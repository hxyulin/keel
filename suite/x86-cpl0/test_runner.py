import json
from pathlib import Path
import struct
import subprocess
import tempfile
import unittest
from unittest import mock

import run


class RunnerTests(unittest.TestCase):
    def test_exact_status_and_transcript_required(self):
        good = run.READY + run.PASS
        self.assertEqual(run.verdict(33, good), 'pass')
        for code, data in [(0, good), (35, good), (-9, good), (33, b''),
                           (33, run.PASS), (33, good + run.FAIL),
                           (33, good + good), (33, good + b'reset')]:
            with self.subTest(code=code, data=data):
                self.assertEqual(run.verdict(code, data), 'guest-error')
        self.assertEqual(run.verdict(33, good, True), 'timeout')

    def test_timeout_after_pass_never_succeeds(self):
        with tempfile.TemporaryDirectory(dir='/tmp') as tmp:
            work = Path(tmp)
            (work / 'debugcon.txt').write_bytes(run.READY + run.PASS)
            with mock.patch.object(run.subprocess, 'run', side_effect=subprocess.TimeoutExpired('qemu', 1)):
                result = run.execute('qemu', work / 'guest.bin', work, 1)
            self.assertEqual(result['status'], 'timeout')
            self.assertIsNone(result['returncode'])

    def test_qemu_error_and_missing_transcript(self):
        with tempfile.TemporaryDirectory(dir='/tmp') as tmp:
            with mock.patch.object(run.subprocess, 'run', return_value=subprocess.CompletedProcess([], 1)):
                result = run.execute('qemu', Path(tmp) / 'guest.bin', Path(tmp), 1)
            self.assertEqual(result['status'], 'guest-error')

    def test_missing_tools_are_errors(self):
        with mock.patch.object(run.shutil, 'which', return_value=None):
            with self.assertRaisesRegex(run.HarnessError, 'missing prerequisite: qemu'):
                run.tool('qemu')

    def test_bad_payload_is_rejected_before_execution(self):
        with tempfile.TemporaryDirectory(dir='/tmp') as tmp:
            path = Path(tmp) / 'bad.o'
            for data in (b'', b'\x7fELF', bytes(64)):
                path.write_bytes(data)
                with self.assertRaises(run.HarnessError):
                    run.validate_payload(path)
            raw = bytearray(64)
            raw[:7] = b'\x7fELF\x02\x01\x01'
            struct.pack_into('<HH', raw, 16, 1, 62)
            path.write_bytes(raw)
            run.validate_payload(path)  # Full ELF integrity is the linker's job.
            struct.pack_into('<H', raw, 16, 2)
            path.write_bytes(raw)
            with self.assertRaises(run.HarnessError):
                run.validate_payload(path)

    def test_build_failure_is_not_a_pass(self):
        with mock.patch.object(run.subprocess, 'run', return_value=subprocess.CompletedProcess([], 1, b'', b'bad object')):
            with self.assertRaisesRegex(run.HarnessError, 'build command failed'):
                run.checked(['ld'])

    def test_flat_header_bounds(self):
        raw = bytearray(64)
        struct.pack_into('<8I', raw, 0, 0x1badb002, 0x10003,
                         (-0x1badb002 - 0x10003) & 0xffffffff,
                         0x100000, 0x100000, 0x100040, 0x101000, 0x100020)
        run.validate_flat(raw)
        for offset in (0, 4, 8, 12, 16, 20, 24, 28):
            bad = raw.copy()
            struct.pack_into('<I', bad, offset, 0)
            with self.assertRaises(run.HarnessError):
                run.validate_flat(bad)
        with self.assertRaises(run.HarnessError):
            run.validate_flat(b'')

    def test_guest_is_emulated_and_disconnected(self):
        argv = run.qemu_command('qemu', Path('/tmp/guest.bin'), Path('/tmp/status'))
        for required in ('tcg,thread=single', '-nodefaults', '-no-user-config', '-no-reboot'):
            self.assertIn(required, argv)
        self.assertEqual(argv[argv.index('-nic') + 1], 'none')
        self.assertNotIn('-no-shutdown', argv)
        for forbidden in ('-enable-kvm', '-drive', '-netdev', '-virtfs', '-fsdev'):
            self.assertNotIn(forbidden, argv)

    def test_cli_errors_return_nonzero(self):
        for args in (['--timeout', '0'], ['--timeout', 'nan'], ['--timeout', '31'],
                     ['--as', '/nonexistent/keel-as']):
            with mock.patch('sys.argv', ['run.py'] + args), mock.patch('builtins.print') as output:
                self.assertEqual(run.main(), 1)
            self.assertEqual(json.loads(output.call_args.args[0])['status'], 'error')


if __name__ == '__main__':
    unittest.main()
