"""Exercise the numerical gate independently of the benchmark implementations."""
from pathlib import Path
import re
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
        self.assertEqual(len(result.stdout.splitlines()), 9)

    def test_nan_output_fails(self):
        result = self.exercise(lambda text: text.replace('a[i] / b[i]', '(0.0f / 0.0f)'))
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('division failed', result.stderr)

    def test_new_kernel_errors_fail(self):
        for function, kernel in [('fm_rsqrt', 'rsqrt'), ('fm_length3', 'length3'),
                                 ('fm_saxpy', 'saxpy'), ('fm_poly', 'polynomial')]:
            with self.subTest(kernel=kernel):
                def corrupt(text):
                    before, body = text.split('void ' + function + '(', 1)
                    body = re.sub(r'out\[i\] = [^;]+;', 'out[i] = 0;', body, count=1)
                    return before + 'void ' + function + '(' + body
                result = self.exercise(corrupt)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(kernel + ' failed', result.stderr)

    def test_new_reduction_errors_fail(self):
        for function, kernel in [('fm_sumsq', 'sumsq'), ('fm_distance2', 'distance2')]:
            with self.subTest(kernel=kernel):
                def corrupt(text):
                    before, body = text.split('float ' + function + '(', 1)
                    return before + 'float ' + function + '(' + body.replace('return sum;', 'return 0;', 1)
                result = self.exercise(corrupt)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(kernel + ' failed', result.stderr)

    def test_wrong_reduction_fails(self):
        result = self.exercise(lambda text: text.replace('return sum;', 'return 0;'))
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('dot failed', result.stderr)


@unittest.skipUnless(shutil.which('clang'), 'Clang required')
class OrderedDriverTests(unittest.TestCase):
    def exercise(self, transform):
        source = Path(__file__).resolve().parent
        with tempfile.TemporaryDirectory() as directory:
            work = Path(directory)
            kernel = work / 'ordered.c'
            kernel.write_text(transform((source / 'ordered.c').read_text()))
            binary = work / 'check'
            subprocess.run(['clang', '-O3', '-fno-fast-math', '-ffp-contract=off',
                            str(source / 'ordered_driver.c'), str(kernel), '-o', str(binary)],
                           check=True, capture_output=True)
            return subprocess.run([str(binary), 'check'], capture_output=True, text=True)

    def test_scalar_passes(self):
        self.assertEqual(self.exercise(lambda s: s).returncode, 0)

    def test_changed_value_fails(self):
        result = self.exercise(lambda s: s.replace('s += p[i]', 's += 0', 1))
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('mismatch', result.stderr)

    def test_reassociated_sum_fails(self):
        def rewrite(s):
            begin = s.index('double sum64(')
            end = s.index('double abs64(', begin)
            return s[:begin] + '''double sum64(const double *p, unsigned long n) {
                double a=0,b=0;
                for(unsigned long i=0;i<n;++i) { if(i&1) b+=p[i]; else a+=p[i]; }
                return a+b;
            }
            ''' + s[end:]
        result = self.exercise(rewrite)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('mismatch', result.stderr)

    def test_tail_overread_faults(self):
        result = self.exercise(lambda s: s.replace('return s;',
                              'volatile double extra=p[n]; (void)extra; return s;', 1))
        self.assertNotEqual(result.returncode, 0)


class OrderedPlacementTests(unittest.TestCase):
    def test_padding_is_deterministic_and_stays_before_functions(self):
        from ordered import padded
        source = '.text\n.globl _sum64\n.p2align 4\n_sum64:\nret\n'
        self.assertEqual(padded(source, 0), source)
        self.assertEqual(padded(source, 1), padded(source, 1))
        result = padded(source, 1)
        self.assertRegex(result, r'\.space \d+\n.globl _sum64')
        self.assertTrue(result.endswith('_sum64:\nret\n'))


if __name__ == '__main__':
    unittest.main()
