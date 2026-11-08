"""Exercise the numerical gate independently of the benchmark implementations."""
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


@unittest.skipUnless(shutil.which('clang'), 'Clang required')
class DriverTests(unittest.TestCase):
    def exercise(self, transform):
        source = Path(__file__).resolve().parent
        with tempfile.TemporaryDirectory() as directory:
            work = Path(directory)
            kernel = work / 'kernel.c'
            kernel.write_text(transform((source / 'scalar.c').read_text()))
            binary = work / 'check'
            subprocess.run(['clang', '-O2', '-fno-fast-math', '-ffp-contract=off',
                            str(source / 'driver.c'), str(kernel), '-lm', '-o', str(binary)],
                           check=True, capture_output=True)
            return subprocess.run([str(binary), '17', '1'], capture_output=True, text=True)

    def test_scalar_passes(self):
        result = self.exercise(lambda text: text)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(len(result.stdout.splitlines()), 3)

    def test_nan_output_fails(self):
        result = self.exercise(lambda text: text.replace('a[i] / b[i]', '(0.0f / 0.0f)'))
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('division failed', result.stderr)

    def test_wrong_reduction_fails(self):
        result = self.exercise(lambda text: text.replace('return sum;', 'return 0;'))
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('dot failed', result.stderr)


if __name__ == '__main__':
    unittest.main()
