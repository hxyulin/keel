"""Small hand-computed gates plus reproducible generator boundary checks."""
import unittest
from oracle import CASES, MASK, inputs, reference


class OracleTests(unittest.TestCase):
    def test_empty(self):
        self.assertEqual(reference([], [], []), dict.fromkeys(CASES, 0))

    def test_hand_computed(self):
        self.assertEqual(reference([-2, 3], [-5, 7], [MASK, 2]), dict(zip(CASES, (
            3, 5, 7, 8, 65538, 257, MASK + 3 & MASK, 5, 7 * MASK + 4 & MASK, 3, 1, (1 << 32) + 1))))

    def test_extremes(self):
        r = reference([-(1 << 31), (1 << 31) - 1], [-(1 << 63), (1 << 63) - 1], [0, MASK])
        self.assertEqual(r['adjacent'], (1 << 32) - 1)
        self.assertEqual(r['balance64'], (1 << 63) + 13)
        self.assertEqual(r['clamped'], 255)

    def test_generator(self):
        a, b, u = inputs(28, 20261021)
        self.assertEqual((a[0], b[0], u[0]), (-(1 << 31), -(1 << 63), 0))
        self.assertEqual(a[24], 128)
        self.assertEqual(a[1:3], [-2109268024, 1847469473])
        self.assertEqual(inputs(28, 20261021), (a, b, u))
        self.assertNotEqual(inputs(28, 17), (a, b, u))


if __name__ == '__main__':
    unittest.main()
