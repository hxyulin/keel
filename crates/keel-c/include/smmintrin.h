/* Bounded Keel intrinsic subset; see docs/c-x86-intrinsics.md. */
#ifndef _KEEL_SMMINTRIN_H
#define _KEEL_SMMINTRIN_H
#ifndef __x86_64__
#error "Keel x86 intrinsic headers require x86-64"
#endif
#include <emmintrin.h>

static __inline__ int _mm_extract_epi32(__m128i a, int i) {
  return ((__keel_i32_128)a)[i & 3];
}
static __inline__ __m128i _mm_insert_epi32(__m128i a, int x, int i) {
  __keel_i32_128 b = (__keel_i32_128)a;
  b[i & 3] = x;
  return (__m128i)b;
}
#endif /* _KEEL_SMMINTRIN_H */
