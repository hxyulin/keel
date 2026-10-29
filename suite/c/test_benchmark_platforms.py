"""Regression tests for extensible application selection and failure reporting."""
import gzip
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

import benchmark_platforms as benchmark


class Preparation(unittest.TestCase):
    def test_prepare_preserves_macro_quotes_and_argument_boundaries(self):
        command = benchmark.prepare_argv(
            'clang -O2 {defines} source.c -o {gen}/tool {libs}',
            lambda s: s.replace('{gen}', '/tmp/generated files'),
            ['-DCONFIG_VERSION="2024-01-13"', '-DNAME=two words'], ['-lm'], '/chosen/clang')
        self.assertEqual(command, ['/chosen/clang', '-O2', '-DCONFIG_VERSION="2024-01-13"',
                                   '-DNAME=two words', 'source.c', '-o', '/tmp/generated files/tool', '-lm'])


class OptimizationMatrix(unittest.TestCase):
    def test_ablation_keeps_optimizer_and_isa_fixed(self):
        matrix = benchmark.optimization_configs('/compiler')
        self.assertEqual(len(matrix), 11)
        self.assertEqual(len(benchmark.optimization_configs('/compiler', True)), 23)
        for name, (kind, compiler, flags) in matrix.items():
            self.assertEqual((kind, compiler), ('keel', '/compiler'))
            self.assertNotIn('--target-features', flags)
            if name.count('-') > 1:
                self.assertIn('-O2', flags)
                self.assertIn('--isel-tier', flags)
                self.assertIn('--regalloc', flags)


@unittest.skipUnless(shutil.which('clang'), 'native Clang required')
class NativeHarness(unittest.TestCase):
    def run_case(self, fail=False, reference_fail=False, input_mode="native"):
        with tempfile.TemporaryDirectory(prefix='keel-app-harness-') as temporary:
            root = Path(temporary)
            app = root / 'suite/c/sample'
            app.mkdir(parents=True)
            (app / 'left').mkdir()
            (app / 'right').mkdir()
            (app / 'left/unit.c').write_text('int left(void) { return VALUE; }\n')
            (app / 'right/unit.c').write_text('int right(void) { return 7; }\n')
            (app / 'driver.c').write_text('#include <stdio.h>\nint left(void); int right(void);\nint main(void) { if (left() != right()) return 9; puts("checked"); return '
                                         + str(int(reference_fail)) + '; }\n')
            (app / 'suite.toml').write_text('''description = "harness integration"
[build]
dir = "."
std = "c11"
sources = ["{suite}/driver.c", "{suite}/left/unit.c", "{suite}/right/unit.c"]
defines = ["VALUE=7"]
binary = "sample"
[test]
run = ["{bin}"]
compare = "clang-O2"
''')
            # This unselected suite must neither download nor prepare sources.
            other = root / 'suite/c/unselected'
            other.mkdir()
            (other / 'suite.toml').write_text('intentionally not a valid selected manifest')
            self.assertEqual(benchmark.discover_apps(root), ['sample', 'unselected'])
            candidate = root / 'candidate.json'
            broken = root / 'broken-keel'
            broken.write_text('#!/bin/sh\nexit 23\n')
            broken.chmod(0o755)
            candidate.write_text(json.dumps({'keel-broken': {'compiler': str(broken), 'flags': []}}))
            output = root / 'result'
            command = [sys.executable, str(Path(benchmark.__file__).resolve()), '--root', str(root),
                       '--work', str(output), '--keel', sys.executable, '--apps', 'sample',
                       '--configs', 'clang-O2', '--checks-only', '--input-mode', input_mode]
            if fail:
                command[command.index('--checks-only'):command.index('--checks-only')] = ['keel-broken']
                command += ['--candidate-configs', str(candidate)]
            result = subprocess.run(command, capture_output=True, text=True, timeout=120)
            self.assertEqual(result.returncode, int(fail or reference_fail), result.stdout + result.stderr)
            report = json.loads((output / 'report.json').read_text())
            self.assertEqual(report['input_mode'], input_mode)
            with gzip.open(output / 'commands.jsonl.gz', 'rt') as log:
                commands = [json.loads(line) for line in log]
            compilation = [row for row in commands if row.get('label') == 'initial-build']
            self.assertEqual(len([row for row in compilation if '-c' in row['argv']]), 3)
            clang_units = [row for row in compilation if '-c' in row['argv']]
            self.assertEqual(len({row['argv'][-1] for row in clang_units}), 3)
            suffix = '.c' if input_mode == 'native' else '.i'
            self.assertTrue(all(any(arg.endswith(suffix) for arg in row['argv']) for row in clang_units))
            if input_mode == 'native':
                self.assertTrue(all('-DVALUE=7' in row['argv'] for row in clang_units))
                self.assertFalse(any(row.get('label') == 'preprocess' for row in commands))
            self.assertEqual(report['complete'], not reference_fail)
            self.assertEqual(set(report['apps']), {'sample'})
            sizes = report['apps']['sample']['builds']['clang-O2']
            self.assertGreater(sizes['text_bytes'], 0)
            self.assertGreaterEqual(sizes['allocated_bytes'], sizes['text_bytes'])
            self.assertLess(sizes['text_bytes'], sizes['bytes'])
            self.assertFalse((root / 'target/suite/unselected').exists())
            self.assertEqual(report['apps']['sample']['compile'], {})
            self.assertEqual(report['apps']['sample']['runtime'], {})
            if reference_fail:
                self.assertFalse(report['apps']['sample']['checks']['clang-O2']['passed'])
                self.assertIn('Clang O2 correctness reference failed', report['fatal_error'])
                return
            self.assertTrue(report['apps']['sample']['checks']['clang-O2']['suite_test_passed'])
            if fail:
                self.assertEqual(report['apps']['sample']['builds']['keel-broken']['status'], 'failed')
                self.assertEqual(len(report['errors']), 1)
            else:
                self.assertEqual(report['errors'], [])

    def test_new_manifest_checks_without_preparing_unselected_suites(self):
        self.run_case(False)

    def test_shared_preprocessing_preserves_translation_units(self):
        self.run_case(input_mode="shared")

    def test_failed_configuration_is_reported_and_returns_failure(self):
        self.run_case(True)

    def test_invalid_clang_oracle_stops_before_timing(self):
        self.run_case(reference_fail=True)


if __name__ == '__main__':
    unittest.main()
