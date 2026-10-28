"""Harness failure semantics; native differential checks live in cpp_coverage.py."""
import contextlib
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import cpp_coverage as coverage


class ImportClassification(unittest.TestCase):
    def test_supported_and_evolving_blockers(self):
        self.assertEqual(coverage.classify_import(0, b'', '', False), 'accepted')
        for diagnostic in ('@f: multiple blocks/PHIs are not in the initial subset',
                           '@f: unsupported instruction opcode 29',
                           '@f: nuw/nsw arithmetic is not in the initial subset',
                           'global data, aliases and ifuncs are not in the initial subset'):
            self.assertEqual(coverage.classify_import(1, b'', 'llvm-import: ' + diagnostic, True), 'rejected')

    def test_failures_are_not_expected_rejections(self):
        for status, stderr in ((-11, 'segfault'), (1, 'Traceback: broken'),
                               (1, 'llvm-import: Keel verification failed: unsupported IR'),
                               (1, 'llvm-import: input.ll:3:4: error: unsupported syntax'),
                               (1, 'llvm-import: library could not be loaded'),
                               (None, '')):
            self.assertNotEqual(coverage.classify_import(status, b'', stderr, True), 'rejected')
        self.assertEqual(coverage.classify_import(1, b'', 'llvm-import: unsupported', False),
                         'destination_preservation_failure')
        self.assertEqual(coverage.classify_import(1, b'partial', 'llvm-import: unsupported', True),
                         'destination_preservation_failure')
        self.assertEqual(coverage.classify_import(0, b'', '', True), 'import_protocol_failure')


class ArtifactSafety(unittest.TestCase):
    def test_unavailable_prerequisite_is_nonzero_and_reported(self):
        with tempfile.TemporaryDirectory() as tmp, contextlib.redirect_stdout(io.StringIO()):
            out = Path(tmp)/'run'
            code = coverage.main(['--libllvm', '/not-used', '--keel', '/not-used',
                                  '--clangxx', '/nonexistent/cpp-coverage-tool', '--out', str(out)])
            self.assertEqual(code, 2)
            report = json.loads((out/'report.json').read_text())
            self.assertEqual(report['status'], 'prerequisite_failure')
            self.assertEqual(report['commands'][0]['failure'], 'unavailable')
            self.assertTrue(Path(report['commands'][0]['stderr']).read_text())

    def test_existing_destination_is_not_modified(self):
        with tempfile.TemporaryDirectory() as tmp, contextlib.redirect_stderr(io.StringIO()):
            sentinel = Path(tmp)/'report.json'
            sentinel.write_text('preserve')
            with self.assertRaises(SystemExit) as raised:
                coverage.main(['--libllvm', '/not-used', '--keel', '/not-used', '--out', tmp])
            self.assertEqual(raised.exception.code, 2)
            self.assertEqual(sentinel.read_text(), 'preserve')

    def test_timeout_retains_output_and_failure(self):
        with tempfile.TemporaryDirectory() as tmp:
            runner = coverage.Runner(Path(tmp), 0.1)
            record = runner.run([sys.executable, '-u', '-c',
                                 'import time; print("before timeout"); time.sleep(5)'])
            self.assertEqual(record['failure'], 'timeout')
            self.assertIsNone(record['returncode'])
            self.assertIn('before timeout', Path(record['stdout']).read_text())


if __name__ == '__main__':
    unittest.main()
