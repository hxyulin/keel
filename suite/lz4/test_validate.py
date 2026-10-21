import pathlib
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from validate import command


class CommandEvidence(unittest.TestCase):
    def test_launch_errors_keep_the_command(self):
        with tempfile.TemporaryDirectory() as directory:
            missing = pathlib.Path(directory) / 'missing'
            row = command([missing], directory)
            self.assertEqual(row['argv'], [str(missing)])
            self.assertEqual(row['cwd'], directory)
            self.assertIsNone(row['returncode'])
            self.assertEqual(row['failure'], 'launch_error')

    def test_timeout_retains_partial_output(self):
        with patch('validate.subprocess.run', side_effect=subprocess.TimeoutExpired(
                ['tool'], 600, output=b'partial stdout', stderr=b'partial stderr')):
            row = command(['tool'])
        self.assertEqual(row['failure'], 'timeout')
        self.assertEqual(row['stdout'], 'partial stdout')
        self.assertEqual(row['stderr'], 'partial stderr')
        self.assertIsNone(row['returncode'])


if __name__ == '__main__':
    unittest.main()
