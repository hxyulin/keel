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
