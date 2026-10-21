"""Failure-path controls: launch errors and internal errors are not rejections."""
import pathlib
import tempfile
import unittest

from check import check
from corpus import generate
from validate import command


class FailureClassification(unittest.TestCase):
    def test_missing_or_nonexecutable_command_is_a_recorded_failure(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = pathlib.Path(temporary)
            nonexecutable = directory / 'not-executable'
            nonexecutable.write_text('not a program')
            for path in [nonexecutable, directory / 'missing']:
                row = command([path])
                self.assertIsNone(row['returncode'])
                self.assertEqual(row['failure'], 'launch_error')
                self.assertEqual(row['argv'], [str(path)])
                self.assertTrue(row['stderr'])

    def test_internal_failure_does_not_count_as_invalid_json_rejection(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = pathlib.Path(temporary)
            corpus = directory / 'corpus'
            generate(corpus)
            fake = directory / 'driver'
            fake.write_text('#!/usr/bin/env python3\nimport json,sys\n'
                            + 'sys.path.insert(0, ' + repr(str(pathlib.Path(__file__).resolve().parent)) + ')\n'
                            + 'from corpus import digest\n'
                            + 'try: value=json.loads(open(sys.argv[1],"rb").read())\n'
                            + 'except (ValueError,UnicodeError):\n'
                            + ' print("roundtrip mismatch",file=sys.stderr);sys.exit(1)\n'
                            + 'print(json.dumps(dict(digest(value),rounds=int(sys.argv[2]))))\n')
            fake.chmod(0o755)
            with self.assertRaises(AssertionError):
                check(str(fake), corpus)


if __name__ == '__main__':
    unittest.main()
