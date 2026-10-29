/* Bounded Keel intrinsic subset; see docs/c-x86-intrinsics.md. */
#ifndef _KEEL_SMMINTRIN_H
#define _KEEL_SMMINTRIN_H
#ifndef __x86_64__
#error "Keel x86 intrinsic headers require x86-64"
#endif
#include <tmmintrin.h>

#define _MM_FROUND_TO_NEAREST_INT 0x00
#define _MM_FROUND_TO_NEG_INF 0x01
#define _MM_FROUND_TO_POS_INF 0x02
#define _MM_FROUND_TO_ZERO 0x03
#define _MM_FROUND_CUR_DIRECTION 0x04
#define _MM_FROUND_RAISE_EXC 0x00
#define _MM_FROUND_NO_EXC 0x08
#define _MM_FROUND_NINT (_MM_FROUND_RAISE_EXC | _MM_FROUND_TO_NEAREST_INT)
#define _MM_FROUND_FLOOR (_MM_FROUND_RAISE_EXC | _MM_FROUND_TO_NEG_INF)
#define _MM_FROUND_CEIL (_MM_FROUND_RAISE_EXC | _MM_FROUND_TO_POS_INF)
#define _MM_FROUND_TRUNC (_MM_FROUND_RAISE_EXC | _MM_FROUND_TO_ZERO)
#define _MM_FROUND_RINT (_MM_FROUND_RAISE_EXC | _MM_FROUND_CUR_DIRECTION)
#define _MM_FROUND_NEARBYINT (_MM_FROUND_NO_EXC | _MM_FROUND_CUR_DIRECTION)

static __inline__ int _mm_extract_epi32(__m128i a, int i) {
  return ((__keel_i32_128)a)[i & 3];
}
static __inline__ __m128i _mm_insert_epi32(__m128i a, int x, int i) {
  __keel_i32_128 b = (__keel_i32_128)a;
  b[i & 3] = x;
  return (__m128i)b;
}

static __inline__ __m128i _mm_min_epi8(__m128i a, __m128i b) {
  __keel_i8_128 x = (__keel_i8_128)a, y = (__keel_i8_128)b;
  __m128i mask = (__m128i)(x < y);
  return (a & mask) | (b & ~mask);
}
static __inline__ __m128i _mm_max_epi8(__m128i a, __m128i b) {
  __keel_i8_128 x = (__keel_i8_128)a, y = (__keel_i8_128)b;
  __m128i mask = (__m128i)(x > y);
  return (a & mask) | (b & ~mask);
}
static __inline__ __m128i _mm_min_epi32(__m128i a, __m128i b) {
  __keel_i32_128 x = (__keel_i32_128)a, y = (__keel_i32_128)b;
  __m128i mask = (__m128i)(x < y);
  return (a & mask) | (b & ~mask);
}
static __inline__ __m128i _mm_max_epi32(__m128i a, __m128i b) {
  __keel_i32_128 x = (__keel_i32_128)a, y = (__keel_i32_128)b;
  __m128i mask = (__m128i)(x > y);
  return (a & mask) | (b & ~mask);
}
static __inline__ __m128i _mm_min_epu16(__m128i a, __m128i b) {
  __keel_u16_128 x = (__keel_u16_128)a, y = (__keel_u16_128)b;
  __m128i mask = (__m128i)(x < y);
  return (a & mask) | (b & ~mask);
}
static __inline__ __m128i _mm_max_epu16(__m128i a, __m128i b) {
  __keel_u16_128 x = (__keel_u16_128)a, y = (__keel_u16_128)b;
  __m128i mask = (__m128i)(x > y);
  return (a & mask) | (b & ~mask);
}
static __inline__ __m128i _mm_min_epu32(__m128i a, __m128i b) {
  __keel_u32_128 x = (__keel_u32_128)a, y = (__keel_u32_128)b;
  __m128i mask = (__m128i)(x < y);
  return (a & mask) | (b & ~mask);
}
static __inline__ __m128i _mm_max_epu32(__m128i a, __m128i b) {
  __keel_u32_128 x = (__keel_u32_128)a, y = (__keel_u32_128)b;
  __m128i mask = (__m128i)(x > y);
  return (a & mask) | (b & ~mask);
}
static __inline__ __m128i _mm_blendv_epi8(__m128i a, __m128i b, __m128i selector) {
  __m128i mask = (__m128i)((__keel_i8_128)selector < (__keel_i8_128){0});
  return (a & ~mask) | (b & mask);
}

#endif /* _KEEL_SMMINTRIN_H */
