"""Public wrapper inventory must ignore names merely mentioned by source."""
import unittest

from vendor_coverage import definitions


class DefinitionsTests(unittest.TestCase):
    def test_definitions_macros_and_calls(self):
        names = definitions('''
        /* _mm_doc(void) { } */
        // #define _mm_comment(x) x
        #define _mm_macro(x) _mm_call(x)
        #define _mm_alias _mm_macro
        static inline __m128 _mm_function(__m128 a) {
          return _mm_called(a);
        }
        static inline __m128 __attribute__((unused)) _mm_same_line(__m128 a) { return a; }
        __m128 _mm_declaration(__m128 a);
        ''')
        self.assertEqual(set(names), {'_mm_macro', '_mm_alias', '_mm_function', '_mm_same_line'})

    def test_mmx_signature_and_adjacent_bodies(self):
        names = definitions('''
        __m64 _mm_legacy(int a) { return (__m64)a; }
        __m128 _mm_mixed(__m64 a) { return a; }
        __m128 _mm_normal(__m128 a) { return a; }
        ''')
        self.assertTrue(names['_mm_legacy'])
        self.assertTrue(names['_mm_mixed'])
        self.assertFalse(names['_mm_normal'])


if __name__ == '__main__':
    unittest.main()
