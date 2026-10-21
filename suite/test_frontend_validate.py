"""Aggregate runner failure/skip contracts without invoking native compilers."""
import contextlib
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import frontend_validate as validation


class FrontendValidation(unittest.TestCase):
    def run_harness(self, system='Darwin', machine='arm64', fail=None, extra=()):
        def fake_run(harness, name, argv, timeout=900):
            harness.stages.append(dict(name=name, status='failed' if name == fail else 'passed'))
            (harness.out / f'{name}.stdout').write_text('/fake/sysroot' if name == 'rust-sysroot' else 'test')
            return name != fail

        with tempfile.TemporaryDirectory() as temporary:
            out = Path(temporary) / 'run'
            binary = Path(temporary) / 'keel'
            binary.write_text('placeholder')
            binary.chmod(0o755)
            with (patch.object(validation.Harness, 'run', fake_run),
                  patch.object(validation.platform, 'system', return_value=system),
                  patch.object(validation.platform, 'machine', return_value=machine),
                  patch.object(validation, 'LOCK', str(Path(temporary) / 'lock')),
                  contextlib.redirect_stdout(io.StringIO())):
                code = validation.main(['--out', str(out), '--keel', str(binary), *extra])
            return code, json.loads((out / 'report.json').read_text())

    def test_all_checks_run_on_supported_host(self):
        code, report = self.run_harness()
        self.assertEqual(code, 0)
        self.assertEqual(report['status'], 'passed')
        self.assertTrue(all(stage['status'] == 'passed' for stage in report['stages']))
        self.assertIn('llvm-import', [stage['name'] for stage in report['stages']])
        self.assertIn('cpp-coverage', [stage['name'] for stage in report['stages']])

    def test_stage_failures_propagate_and_other_checks_continue(self):
        for name in ('harness-tests', 'c-attributes', 'c-preprocess', 'c-native',
                     'llvm-import', 'llvm-calls', 'cpp-coverage'):
            with self.subTest(stage=name):
                code, report = self.run_harness(fail=name)
                self.assertEqual(code, 1)
                self.assertEqual(report['status'], 'failed')
                self.assertIn('cpp-coverage', [stage['name'] for stage in report['stages']])

    def test_discovery_failure_cannot_be_a_successful_skip(self):
        code, report = self.run_harness(fail='rust-sysroot')
        self.assertEqual(code, 1)
        self.assertEqual(report['status'], 'failed')

    def test_unsupported_host_reports_explicit_skips(self):
        code, report = self.run_harness(system='Linux', machine='x86_64')
        self.assertEqual(code, 0)
        self.assertEqual(report['status'], 'passed_with_skips')
        skips = [stage['name'] for stage in report['stages'] if stage['status'] == 'skipped']
        self.assertEqual(skips, ['llvm-import', 'llvm-calls', 'cpp-coverage'])
        code, report = self.run_harness(system='Linux', machine='x86_64', extra=['--require-llvm'])
        self.assertEqual(code, 1)
        self.assertEqual(report['status'], 'failed')

    def test_existing_artifacts_survive(self):
        with tempfile.TemporaryDirectory() as temporary, contextlib.redirect_stderr(io.StringIO()):
            sentinel = Path(temporary) / 'report.json'
            sentinel.write_text('preserve')
            with self.assertRaises(SystemExit) as error:
                validation.main(['--out', temporary])
            self.assertEqual(error.exception.code, 2)
            self.assertEqual(sentinel.read_text(), 'preserve')


if __name__ == '__main__':
    unittest.main()
