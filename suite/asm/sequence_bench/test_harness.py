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

    def test_extension_oracles_at_signed_and_wrapping_boundaries(self):
        for case, bits in (run.MASK_WIDTHS | run.SIGN_WIDTHS | run.ZERO_SHIFT_WIDTHS).items():
            for seed in run.validation_seeds(case, 1):
                for count in (0, 1, 2, 7, 255):
                    value = seed
                    for _ in range(count):
                        raw = ((value + 17) & run.MASK).to_bytes(8, 'little')[:bits // 8]
                        value = int.from_bytes(raw, 'little', signed=case in run.SIGN_WIDTHS) & run.MASK
                    self.assertEqual(run.oracle(case, 1, 1, count, seed), value, (case, seed, count))

    def test_folded_signed_load_retains_signed_result_and_unsigned_index(self):
        for seed in (0, 3, 127, 128, run.MASK):
            value = seed & 255
            for count in (0, 1, 2, 7, 255, 256):
                expected = value
                for _ in range(count):
                    byte = ((expected & 255) * 73 + 19) & 255
                    expected = int.from_bytes(bytes([byte]), 'little', signed=True) & run.MASK
                self.assertEqual(run.oracle('fold_signed_load_byte', 1, 1, count, seed), expected)
                self.assertEqual(run.oracle('fold_load_byte', 1, 1, count, seed), expected & 255)

    def test_zero_consumers_keep_both_outcomes(self):
        for seed in (0, 1, 2, 1 << 32, run.MASK):
            for count in (0, 1, 2, 7, 8):
                setcc = seed
                branch = seed & 1
                for _ in range(count):
                    setcc = int(setcc == 0)
                    branch = branch - 1 if branch else 1
                self.assertEqual(run.oracle('zero_setcc', 1, 1, count, seed), setcc)
                self.assertEqual(run.oracle('zero_branch', 1, 1, count, seed), branch)
                for case in run.ZERO_PREDICATES:
                    value = seed & ((1 << 32) - 1) if case == 'zero_setcc32' else seed
                    for _ in range(count):
                        value = int(value == 0)
                    self.assertEqual(run.oracle(case, 1, 1, count, seed), value)
        asm, _, _ = run.source('zero_branch', True, 8, 16)
        labels = [line for line in asm.splitlines() if line.endswith(':')]
        self.assertEqual(len(labels), len(set(labels)))
        self.assertEqual(len(labels), 8 * 16 + 2)

    def test_wide_load_oracles_include_sign_bits_and_truncation(self):
        self.assertEqual(run.wide_value(0), 0x07000313)
        self.assertEqual(run.wide_value(255), 0xfa00f8ca)
        memory = b''.join(run.wide_value(i).to_bytes(4, 'little') for i in range(256))
        for case, (bits, signed) in run.WIDE_LOADS.items():
            for seed in (0, 1, 127, 128, 255, run.MASK):
                for count in (0, 1, 2, 7, 255, 256):
                    value = seed & 255
                    for _ in range(count):
                        offset = (value & 255) * 4
                        value = int.from_bytes(memory[offset:offset + bits // 8],
                                               'little', signed=signed) & run.MASK
                    self.assertEqual(run.oracle(case, 1, 1, count, seed), value, (case, seed, count))

    def test_boolean_and_mask_materialization_oracles(self):
        operations = {'bool_nonzero': lambda v: int(v != 0),
                      'mask_nonzero': lambda v: -int(v != 0) & run.MASK,
                      'mask_zero': lambda v: -int(v == 0) & run.MASK,
                      'invert_boolean': lambda v: int(v == 0)}
        for case, op in operations.items():
            for seed in (0, 1, 2, run.MASK):
                for count in (0, 1, 2, 7, 8):
                    value = seed & 1 if case == 'invert_boolean' else seed
                    for _ in range(count):
                        value = op(value)
                    self.assertEqual(run.oracle(case, 1, 1, count, seed), value)

    def test_rotate_period_and_arithmetic_oracles(self):
        for case in ('rotate_left', 'rotate_right'):
            for seed in (0, 1, run.MASK, 0x81abcdef01234567):
                for count in (0, 1, 2, 63, 64, 65):
                    text = f'{seed:064b}'
                    for _ in range(count):
                        text = text[13:] + text[:13] if case == 'rotate_left' else text[-13:] + text[:-13]
                    self.assertEqual(run.oracle(case, 1, 1, count, seed), int(text, 2))
        operations = {'bitwise_not': lambda v: ~v, 'negate': lambda v: -v,
                      'add_lea': lambda v: v + 17, 'double': lambda v: v * 2,
                      'increment': lambda v: v + 1, 'loop_dec_sub': lambda v: v + 17}
        for case, op in operations.items():
            for seed in (0, 1, run.MASK, 1 << 63):
                for count in (0, 1, 2, 64, 65):
                    value = seed
                    for _ in range(count):
                        value = op(value) & run.MASK
                    self.assertEqual(run.oracle(case, 1, 1, count, seed), value)

    def test_absolute_recurrence_at_reflection_boundaries(self):
        boundary = 1 << 63
        for seed in (0, run.MASK, boundary, boundary - 1, boundary - 16,
                     boundary - 17, boundary - 18, 1 << 62):
            for count in (0, 1, 2, 7, 17, 64, 257):
                value = seed
                for _ in range(count):
                    raw = ((value + 17) & run.MASK).to_bytes(8, 'little')
                    value = abs(int.from_bytes(raw, 'little', signed=True)) & run.MASK
                self.assertEqual(run.absolute_recurrence(seed, count), value)
        self.assertEqual(run.absolute_recurrence(boundary - 17, 1 << 40), boundary - 17)
        self.assertEqual(run.absolute_recurrence(boundary - 17, (1 << 40) + 1), boundary)

    def test_invalid_tsc_and_affinity_samples_are_rejected(self):
        valid = {'checksum': 73, 'cpu': 4, 'locked': True, 'ns': 1000,
                 'tsc_ticks': 2400, 'tsc_aux_start': 4, 'tsc_aux_end': 4}
        run.validate_sample(valid, 73, 4, 'dual')
        run.validate_sample({k: v for k, v in valid.items() if not k.startswith('tsc_')}, 73, 4, 'wall')
        for bad in ({'tsc_ticks': 0}, {'tsc_aux_end': 5}, {'cpu': 5}, {'locked': False},
                    {'checksum': 0}, {'ns': 0}):
            with self.assertRaises(ValueError):
                run.validate_sample(valid | bad, 73, 4, 'dual')

    def test_multiply_oracle_wraps_and_reaches_zero_for_even_factor(self):
        for case, factor in run.MULTIPLIERS.items():
            for seed in (0, 1, run.MASK, 1 << 63):
                for count in (0, 1, 2, 31, 64, 65):
                    value = seed
                    for _ in range(count):
                        value = value * factor & run.MASK
                    self.assertEqual(run.oracle(case, 1, 1, count, seed), value)

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
