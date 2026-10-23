import unittest
from run import paired_order

class Pairing(unittest.TestCase):
    def test_every_round_has_both_arms_and_seed_is_reproducible(self):
        a = list(paired_order(20261023, 9))
        self.assertEqual(a, list(paired_order(20261023, 9)))
        self.assertEqual([r for r, _ in a], list(range(9)))
        self.assertTrue(all(sorted(arms) == [128, 256] for _, arms in a))
        self.assertEqual({tuple(arms) for _, arms in a}, {(128, 256), (256, 128)})

class Disassembly(unittest.TestCase):
    def test_continuation_bytes_and_next_symbol_are_not_instructions(self):
        from audit import instructions
        text = '''0000000000000000 <rk_widen32>:
   0:\t48 bf ff ff ff ff ff \tmovabs rdi,0x3fffffffffffffff
   7:\tff ff 3f
   a:\tc3                   \tret

0000000000000010 <other>:
  10:\tc3                   \tret
'''
        rows = instructions(text, 'rk_widen32')
        self.assertEqual([r['mnemonic'] for r in rows], ['movabs', 'ret'])
        self.assertEqual([r['address'] for r in rows], [0, 10])

class Provenance(unittest.TestCase):
    def test_frozen_compiler_and_helpers_cannot_change(self):
        import tempfile
        from pathlib import Path
        from provenance import blob_id, verify, EXAMPLE
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            helper = 'suite/width-comparison/compiler.rs'
            for name, data in [('Cargo.lock', b'locked'), (helper, b'compiler'), (EXAMPLE, b'compiler')]:
                (root/name).parent.mkdir(parents=True, exist_ok=True)
                (root/name).write_bytes(data)
            manifest = dict(base='8'*40, blobs={'Cargo.lock': blob_id(b'locked')},
                            overlays={helper: blob_id(b'compiler'), EXAMPLE: blob_id(b'compiler')})
            self.assertEqual(verify(root, manifest), '8'*40)
            (root/'Cargo.lock').write_bytes(b'sibling change')
            with self.assertRaisesRegex(ValueError, 'frozen base changed'):
                verify(root, manifest)
            (root/'Cargo.lock').write_bytes(b'locked')
            (root/EXAMPLE).write_bytes(b'other compiler')
            with self.assertRaisesRegex(ValueError, 'helper changed'):
                verify(root, manifest)
            manifest['overlays']['crates/keel-opt/src/lib.rs'] = blob_id(b'policy')
            with self.assertRaisesRegex(ValueError, 'overlay outside'):
                verify(root, manifest)
