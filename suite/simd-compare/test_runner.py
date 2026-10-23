"""Deterministic safety/oracle tests; no benchmark campaign in normal tests."""
import importlib.util
import json
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import tempfile
import unittest

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('simd_runner', HERE/'run.py')
r = importlib.util.module_from_spec(spec)
spec.loader.exec_module(r)


class SafetyTests(unittest.TestCase):
    def test_paired_campaign_rejects_unmatched_tiers_and_insufficient_rounds(self):
        with tempfile.TemporaryDirectory() as tmp:
            common = [sys.executable, str(HERE/'run.py'), '--keel', sys.executable,
                      '--baseline-keel', sys.executable, '--output', str(Path(tmp)/'new')]
            for flags, diagnostic in [
                    ([], 'requires --variants full'),
                    (['--variants', 'full', '--benchmark', '--rounds', '6'],
                     'at least seven rounds')]:
                result = subprocess.run([*common, *flags], capture_output=True, text=True)
                self.assertEqual(result.returncode, 2)
                self.assertIn(diagnostic, result.stderr)
                self.assertFalse((Path(tmp)/'new').exists())

    def test_features_and_foreign_targets_fail_closed(self):
        self.assertFalse(r.native_allowed('v3', 'Darwin', 'arm64', {'avx2'}))
        self.assertFalse(r.native_allowed('v3', 'Linux', 'x86_64', {'sse2', 'avx2'}))
        self.assertTrue(r.native_allowed('v1', 'Linux', 'x86_64', {'sse2'}))
        self.assertFalse(r.native_allowed('unknown', 'Linux', 'x86_64', {'sse2'}))
        self.assertFalse(r.native_allowed('v1', 'Linux', 'x86_64', {'sse2'}, True))
        v2 = {'sse2', 'pni', 'ssse3', 'sse4_1', 'sse4_2', 'popcnt', 'cx16', 'lahf_lm'}
        self.assertTrue(r.native_allowed('v2', 'Linux', 'x86_64', v2))
        v3 = v2 | {'avx', 'avx2', 'bmi1', 'bmi2', 'f16c', 'fma', 'abm', 'movbe', 'xsave'}
        self.assertTrue(r.native_allowed('v3', 'Linux', 'x86_64', v3))
        for flag in v3:
            self.assertFalse(r.native_allowed('v3', 'Linux', 'x86_64', v3-{flag}))
        self.assertFalse(r.native_allowed('apple-arm64', 'Darwin', 'arm64', [], True))
        self.assertTrue(r.native_allowed('apple-arm64', 'Darwin', 'arm64', []))

    def test_oracle_hand_computed_edges(self):
        unpack = lambda case: struct.unpack('<12I', r.corpus(case)[:48])
        self.assertEqual(unpack('add')[8:], (1, 0x80000001, 0x55555554, 0x80000000))
        self.assertEqual(unpack('compare')[8:], (0xffffffff, 0, 0xffffffff, 0xffffffff))
        self.assertEqual(unpack('shuffle')[8:], (0x80000000, 1, 1, 0x55555555))
        self.assertEqual(unpack('reduce')[8:], (0x80000000,)*4)
        self.assertEqual(struct.unpack('<2q', r.corpus('widen')[32:48]), (0, 1))
        # Second row widens 1 and -1, preserving signedness.
        self.assertEqual(struct.unpack('<2q', r.corpus('widen')[80:96]), (1, -1))
        for case in r.CASES:
            self.assertEqual(len(r.corpus(case)), 64*48)
            self.assertEqual(r.corpus(case), r.corpus(case))

    def test_selection_case_oracles_all_lanes(self):
        # Distinct hand-computed words expose lane duplication/source mistakes.
        self.assertEqual(struct.unpack('<4I', r.corpus('permute-dword')[32:48]),
                         (0xffffffff, 0, 0x80000000, 1))
        self.assertEqual(struct.unpack('<4I', r.corpus('shuffle-pairs')[32:48]),
                         (1, 0, 0, 0x55555555))
        data = r.corpus('compare-u8')
        expected = bytes([255, 0, 255, 0, 0, 0, 0, 0] * 2)
        for row in range(8):
            self.assertEqual(data[row*48+32:row*48+48],
                             expected[row:] + expected[:row])
        # Every byte lane sees both directions across the sign boundary,
        # extrema, and equality at each boundary value.
        for lane in range(16):
            observed = {(data[row*48+lane], data[row*48+16+lane]) for row in range(8)}
            self.assertEqual(observed, {(0, 255), (255, 0), (127, 128), (128, 127),
                                        (0, 0), (127, 127), (128, 128), (255, 255)})

    def test_expected_rejection_cannot_hide_a_crash_or_unrelated_error(self):
        width = 'requires a backend with 256-bit SIMD value support'
        arm_c = '32-byte C vectors require the x86-64 target and AVX/AVX2 code generation'
        for route in ('c', 'ir'):
            for profile in r.PROFILES:
                with self.subTest(route=route, profile=profile):
                    diagnostic = arm_c if route == 'c' and profile == 'apple-arm64' else width
                    expected = profile == 'v3'
                    for code in (-11, None):
                        self.assertFalse(r.probe_matches(route, profile,
                            {'returncode': code, 'stderr': diagnostic}))
                    self.assertFalse(r.probe_matches(route, profile,
                        {'returncode': 1, 'stderr': 'unknown option'}))
                    self.assertEqual(r.probe_matches(route, profile,
                        {'returncode': 0, 'stderr': ''}), expected)
                    self.assertEqual(r.probe_matches(route, profile,
                        {'returncode': 1, 'stderr': diagnostic}), not expected)
        self.assertFalse(r.probe_matches('c', 'apple-arm64',
            {'returncode': 1, 'stderr': width}))

    def test_stack_references_are_not_claimed_as_spills(self):
        result = r.inspect_assembly('kernel:\n  sub sp, sp, #32\n  str q0, [sp]\n  add v0.4s, v0.4s, v1.4s\n  ret\n')
        self.assertIsNone(result['confirmed_spills'])
        self.assertEqual(len(result['stack_reference_candidates']), 2)
        self.assertEqual(result['assembly_opcode_counts']['add'], 1)

    @unittest.skipUnless(shutil.which('clang'), 'existing Clang required')
    def test_real_driver_accepts_all_oracles_and_rejects_corruption(self):
        with tempfile.TemporaryDirectory() as tmp:
            binary = Path(tmp)/'check'
            for case in r.CASES:
                subprocess.run(['clang', '-O2', '-ffp-contract=off', str(HERE/'driver.c'),
                                str(HERE/(case+'.c')), '-o', str(binary)], check=True, capture_output=True)
                data = r.corpus(case)
                result = subprocess.run([str(binary), '1'], input=data, capture_output=True)
                self.assertEqual(result.returncode, 0, (case, result.stderr))
                self.assertEqual(json.loads(result.stdout)['calls'], 64)
                damaged = bytearray(data); damaged[32] ^= 1
                self.assertEqual(subprocess.run([str(binary), '1'], input=damaged,
                                               capture_output=True).returncode, 3)
            (Path(tmp)/'bad.c').write_text('#include <string.h>\nvoid kernel(void*d,const void*a,const void*b){memcpy(d,a,16);((char*)d)[16]=0;}')
            subprocess.run(['clang', str(HERE/'driver.c'), str(Path(tmp)/'bad.c'), '-o', str(binary)], check=True)
            self.assertEqual(subprocess.run([str(binary), '1'], input=r.corpus('copy'),
                                           capture_output=True).returncode, 4)


if __name__ == '__main__':
    unittest.main()
