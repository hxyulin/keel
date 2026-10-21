import tempfile
import unittest
from pathlib import Path
from run import compiler_configurations, canonical_runtime


class RunnerTests(unittest.TestCase):
    def test_profiles_select_optimized_codegen_independently_of_ir_tier(self):
        configs = compiler_configurations()
        for tier in ('none', 'fast', 'full'):
            family, flags = configs['keel-' + tier]
            self.assertEqual(family, 'keel')
            self.assertEqual('-O' in flags, tier != 'none')
            self.assertEqual(flags[flags.index('--opt-tier') + 1], tier)
        self.assertNotIn('gcc-O2', configs)
        self.assertIn('gcc-O2', compiler_configurations(True))

    def test_canonical_path_switches_inodes_without_mutating_retained_binaries(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            a, b = root / 'short', root / 'long-configuration-name'
            a.write_bytes(b'first')
            b.write_bytes(b'second')
            canonical = canonical_runtime(a, root)
            self.assertEqual(canonical_runtime(a, root), canonical)
            self.assertFalse((root / 'run.next').exists())
            first_inode = canonical.stat().st_ino
            self.assertEqual(canonical.read_bytes(), b'first')
            self.assertEqual(canonical_runtime(b, root), canonical)
            self.assertNotEqual(canonical.stat().st_ino, first_inode)
            self.assertEqual(canonical.read_bytes(), b'second')
            self.assertEqual(canonical_runtime(b, root), canonical)
            self.assertEqual(a.read_bytes(), b'first')
            self.assertEqual(b.read_bytes(), b'second')
            self.assertFalse((root / 'run.next').exists())


if __name__ == '__main__':
    unittest.main()
