"""Adversarial harness checks; run with python3 -B -m unittest discover ... ."""
import importlib.util
import json
import pathlib
import shutil
import subprocess
import tempfile
import unittest

HERE = pathlib.Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('arm_runner', HERE/'run.py')
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


class EvidenceTests(unittest.TestCase):
    def test_mutation_of_source_and_binary_is_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            for name in ('source.i', 'binary'):
                path = pathlib.Path(tmp)/name
                path.write_bytes(b'original')
                identities = {str(path): runner.sha(path)}
                runner.verify(identities)
                path.write_bytes(b'mutated')
                with self.assertRaisesRegex(RuntimeError, 'identity changed'):
                    runner.verify(identities)

    def test_masks_use_width_and_immediate_fields_not_registers(self):
        # AND X0,X0,#0xff and AND W0,W0,#0xff; changing registers is harmless.
        self.assertEqual(runner.logical(0x92401c00), ['and', 64, '0xff'])
        self.assertEqual(runner.logical(0x12001c00), ['and', 32, '0xff'])
        self.assertEqual(runner.logical(0x92401c63), ['and', 64, '0xff'])
        self.assertIsNone(runner.logical(0xd65f03c0))  # RET

    def test_zero_work_cannot_be_a_measurement(self):
        case = {'id': 4, 'expected_logical': ['and', 64, '0xff']}
        shape = runner.inspect(bytes.fromhex('000080d2c0035fd6'),
                               '0000000000000000 mov x0, #0\n0000000000000004 ret\n', case)
        self.assertFalse(shape['measurement_eligible'])
        self.assertFalse(shape['expected_shape_present'])

    @unittest.skipUnless(shutil.which('clang'), 'requires existing Clang; no install')
    def test_real_driver_rejects_wrong_checksum_and_zero_work_kernel(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = pathlib.Path(tmp)
            source, binary = tmp/'kernel.c', tmp/'driver'
            source.write_text('unsigned long long kernel(const unsigned long long*p,unsigned long n){return n;}')
            subprocess.run(['clang', '-O2', str(HERE/'driver.c'), str(source), '-o', str(binary)], check=True)
            control = {'id': 0}
            runner.sample(binary, control, 7, 10000, 1761)
            with self.assertRaisesRegex(RuntimeError, 'exit 4'):
                runner.sample(binary, control, 7, 10000, 1761, poison=True)
            with self.assertRaisesRegex(RuntimeError, 'exit 3'):
                runner.sample(binary, {'id': 4}, 7, 10000, 1761)
            # A fabricated record with zero time cannot pass parser validation.
            from unittest.mock import patch
            row = dict(seconds=0, checksum='0', single='0', n=7, repeat=3)
            with patch.object(runner, 'command', return_value=json.dumps(row)):
                with self.assertRaisesRegex(RuntimeError, 'invalid timer'):
                    runner.sample(binary, control, 7, 10000, 1761)


if __name__ == '__main__':
    unittest.main()
