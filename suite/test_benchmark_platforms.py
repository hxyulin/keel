"""Regression tests for extensible application selection and failure reporting."""
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


@unittest.skipUnless(shutil.which('clang'), 'native Clang required')
class NativeHarness(unittest.TestCase):
    def run_case(self, fail=False, reference_fail=False):
        with tempfile.TemporaryDirectory(prefix='keel-app-harness-') as temporary:
            root = Path(temporary)
            app = root / 'suite/sample'
            app.mkdir(parents=True)
            (app / 'driver.c').write_text('#include <stdio.h>\nint main(void) { puts("checked"); return '
                                         + str(int(reference_fail)) + '; }\n')
            (app / 'suite.toml').write_text('''description = "harness integration"
[build]
dir = "."
std = "c11"
sources = ["{suite}/driver.c"]
binary = "sample"
[test]
run = ["{bin}"]
compare = "clang-O2"
''')
            # This unselected suite must neither download nor prepare sources.
            other = root / 'suite/unselected'
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
                       '--configs', 'clang-O2', '--checks-only']
            if fail:
                command[command.index('--checks-only'):command.index('--checks-only')] = ['keel-broken']
                command += ['--candidate-configs', str(candidate)]
            result = subprocess.run(command, capture_output=True, text=True, timeout=120)
            self.assertEqual(result.returncode, int(fail or reference_fail), result.stdout + result.stderr)
            report = json.loads((output / 'report.json').read_text())
            self.assertEqual(report['complete'], not reference_fail)
            self.assertEqual(set(report['apps']), {'sample'})
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

    def test_failed_configuration_is_reported_and_returns_failure(self):
        self.run_case(True)

    def test_invalid_clang_oracle_stops_before_timing(self):
        self.run_case(reference_fail=True)


if __name__ == '__main__':
    unittest.main()
