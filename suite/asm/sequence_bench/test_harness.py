import importlib.util
from pathlib import Path
import struct
import unittest

spec = importlib.util.spec_from_file_location('sequence_run', Path(__file__).with_name('run.py'))
run = importlib.util.module_from_spec(spec)
spec.loader.exec_module(run)


class SequenceTests(unittest.TestCase):
    def test_affine_matches_iterative_wrapping_oracle(self):
        for seed in (0, 1, run.MASK, 1 << 63):
            for count in (0, 1, 2, 3, 7, 8, 15, 16, 255, 256, 257):
                value = seed
                for _ in range(count):
                    value = (value * 3 + 7) & run.MASK
                self.assertEqual(run.affine(seed, count), value)

    def test_load_permutation_and_compare_boundaries(self):
        for seed in (0, 3, 255, 256, run.MASK):
            for count in (0, 1, 2, 255, 256, 257, 511):
                value = seed & 255
                for _ in range(count):
                    value = (value * 73 + 19) & 255
                self.assertEqual(run.oracle('load_extension', 1, 1, count, seed), value)
            self.assertEqual(run.oracle('compare_width', 1, 1, 0, seed), seed)
            self.assertEqual(run.oracle('compare_width', 1, 1, 1, seed), int(seed & 255 == 3))
            self.assertEqual(run.oracle('compare_width', 1, 1, 2, seed), 0)

    def test_checksum_does_not_cancel_equal_streams(self):
        self.assertNotEqual(run.oracle('constant_materialization', 8, 16, 1, 0), 0)
        self.assertEqual(run.oracle('constant_materialization', 1, 16, 1, 0), run.CONSTANT)

    def test_null_control_and_paired_units(self):
        self.assertEqual(run.interval([(10, 10), (1000, 1000)])['ci95_percent'], [0, 0])
        result = run.interval([(10, 11), (1000, 1100)])
        self.assertAlmostEqual(result['change_percent'], 10)
        self.assertAlmostEqual(result['ci95_percent'][0], 10)
        with self.assertRaises(ValueError):
            run.interval([(1, 1)])

    def test_loop_body_excludes_wrapper_and_has_exact_unroll(self):
        asm, first, end = run.source('load_extension', False, 8, 16)
        body = asm.splitlines()[first - 1:end - 1]
        self.assertEqual(len(body), 8 * 16 * 2)
        self.assertEqual(body.count('movzx esi, byte ptr [rdx + rsi]'), 16)
        self.assertNotIn('push r12', body)
        with self.assertRaises(ValueError):
            run.source('load_extension', False, 9, 16)

    def test_metric_mismatch_and_relocation_rejected(self):
        region = {'section': 0, 'relocations': [], 'kind': 'instruction',
                  'offset': 0, 'size': 1, 'bytes': [0xc3], 'line': 4}
        report = {'sections': [{'name': '.text', 'index': 0}], 'regions': [region]}
        self.assertEqual(run.verify_report(report, b'\xc3', 4, 5)['body_bytes'], 1)
        with self.assertRaisesRegex(ValueError, 'bytes differ'):
            run.verify_report(report, b'\x90', 4, 5)
        region['relocations'] = [{'offset': 0}]
        with self.assertRaisesRegex(ValueError, 'relocations'):
            run.verify_report(report, b'\xc3', 4, 5)

    def test_elf_reader_rejects_relocations_and_truncation(self):
        data = bytearray(64 + 4 * 64)
        data[:6] = b'\x7fELF\x02\x01'
        struct.pack_into('<HH', data, 16, 1, 62)
        struct.pack_into('<Q', data, 40, 64)
        struct.pack_into('<HHH', data, 58, 64, 4, 1)
        names = b'\0.shstrtab\0.text\0.rela.text\0'
        start = len(data)
        data.extend(names + b'\xc3' + bytes(24))
        def header(index, name, kind, offset, length, info=0):
            struct.pack_into('<IIQQQQIIQQ', data, 64 + index * 64,
                             name, kind, 4 if kind == 1 else 0, 0, offset, length, 0, info, 1, 0)
        header(1, 1, 3, start, len(names))
        header(2, 11, 1, start + len(names), 1)
        self.assertEqual(run.text_section(data), b'\xc3')
        header(3, 17, 4, len(data) - 24, 24, 2)
        with self.assertRaisesRegex(ValueError, 'relocations'):
            run.text_section(data)
        with self.assertRaises(ValueError):
            run.text_section(data[:100])


if __name__ == '__main__':
    unittest.main()
