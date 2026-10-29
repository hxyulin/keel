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

static __inline__ __attribute__((always_inline)) int _mm_extract_epi32(__m128i a, int i) {
  return ((__keel_i32_128)a)[i & 3];
}
static __inline__ __attribute__((always_inline)) __m128i _mm_insert_epi32(__m128i a, int x, int i) {
  __keel_i32_128 b = (__keel_i32_128)a;
  b[i & 3] = x;
  return (__m128i)b;
}

static __inline__ __attribute__((always_inline)) __m128i _mm_min_epi8(__m128i a, __m128i b) {
  __keel_i8_128 x = (__keel_i8_128)a, y = (__keel_i8_128)b;
  __m128i mask = (__m128i)(x < y);
  return (a & mask) | (b & ~mask);
}
static __inline__ __attribute__((always_inline)) __m128i _mm_max_epi8(__m128i a, __m128i b) {
  __keel_i8_128 x = (__keel_i8_128)a, y = (__keel_i8_128)b;
  __m128i mask = (__m128i)(x > y);
  return (a & mask) | (b & ~mask);
}
static __inline__ __attribute__((always_inline)) __m128i _mm_min_epi32(__m128i a, __m128i b) {
  __keel_i32_128 x = (__keel_i32_128)a, y = (__keel_i32_128)b;
  __m128i mask = (__m128i)(x < y);
  return (a & mask) | (b & ~mask);
}
static __inline__ __attribute__((always_inline)) __m128i _mm_max_epi32(__m128i a, __m128i b) {
  __keel_i32_128 x = (__keel_i32_128)a, y = (__keel_i32_128)b;
  __m128i mask = (__m128i)(x > y);
  return (a & mask) | (b & ~mask);
}
static __inline__ __attribute__((always_inline)) __m128i _mm_min_epu16(__m128i a, __m128i b) {
  __keel_u16_128 x = (__keel_u16_128)a, y = (__keel_u16_128)b;
  __m128i mask = (__m128i)(x < y);
  return (a & mask) | (b & ~mask);
}
static __inline__ __attribute__((always_inline)) __m128i _mm_max_epu16(__m128i a, __m128i b) {
  __keel_u16_128 x = (__keel_u16_128)a, y = (__keel_u16_128)b;
  __m128i mask = (__m128i)(x > y);
  return (a & mask) | (b & ~mask);
}
static __inline__ __attribute__((always_inline)) __m128i _mm_min_epu32(__m128i a, __m128i b) {
  __keel_u32_128 x = (__keel_u32_128)a, y = (__keel_u32_128)b;
  __m128i mask = (__m128i)(x < y);
  return (a & mask) | (b & ~mask);
}
static __inline__ __attribute__((always_inline)) __m128i _mm_max_epu32(__m128i a, __m128i b) {
  __keel_u32_128 x = (__keel_u32_128)a, y = (__keel_u32_128)b;
  __m128i mask = (__m128i)(x > y);
  return (a & mask) | (b & ~mask);
}
static __inline__ __attribute__((always_inline)) __m128i _mm_blendv_epi8(__m128i a, __m128i b, __m128i selector) {
  __m128i mask = (__m128i)((__keel_i8_128)selector < (__keel_i8_128){0});
  return (a & ~mask) | (b & mask);
}

static __inline__ __attribute__((always_inline)) __m128i _mm_packus_epi32(__m128i a, __m128i b) {
  return (__m128i)__builtin_keel_sse41_packusdw128((__keel_i32_128)a, (__keel_i32_128)b);
}

static __inline__ __attribute__((always_inline)) __m128i _mm_mul_epi32(__m128i a, __m128i b) {
  return (__m128i)__builtin_keel_sse41_pmuldq128((__keel_i32_128)a, (__keel_i32_128)b);
}

#define _mm_round_ps(a, mode) __builtin_keel_sse41_roundps128((__m128)(a), (mode))
static __inline__ __attribute__((always_inline)) __m128 _mm_ceil_ps(__m128 a) {
  return _mm_round_ps(a, _MM_FROUND_CEIL);
}
static __inline__ __attribute__((always_inline)) __m128 _mm_floor_ps(__m128 a) {
  return _mm_round_ps(a, _MM_FROUND_FLOOR);
}
#define _mm_round_pd(a, mode) __builtin_keel_sse41_roundpd128((__m128d)(a), (mode))
static __inline__ __attribute__((always_inline)) __m128d _mm_ceil_pd(__m128d a) {
  return _mm_round_pd(a, _MM_FROUND_CEIL);
}
static __inline__ __attribute__((always_inline)) __m128d _mm_floor_pd(__m128d a) {
  return _mm_round_pd(a, _MM_FROUND_FLOOR);
}
#define _mm_round_ss(a, b, mode) \
  __extension__({ \
    __m128 __keel_round_a = (a), __keel_round_b = (b); \
    __keel_round_a[0] = __builtin_keel_sse41_roundss(__keel_round_b[0], (mode)); \
    __keel_round_a; \
  })
static __inline__ __attribute__((always_inline)) __m128 _mm_ceil_ss(__m128 a, __m128 b) {
  return _mm_round_ss(a, b, _MM_FROUND_CEIL);
}
static __inline__ __attribute__((always_inline)) __m128 _mm_floor_ss(__m128 a, __m128 b) {
  return _mm_round_ss(a, b, _MM_FROUND_FLOOR);
}
#define _mm_round_sd(a, b, mode) \
  __extension__({ \
    __m128d __keel_round_a = (a), __keel_round_b = (b); \
    __keel_round_a[0] = __builtin_keel_sse41_roundsd(__keel_round_b[0], (mode)); \
    __keel_round_a; \
  })
static __inline__ __attribute__((always_inline)) __m128d _mm_ceil_sd(__m128d a, __m128d b) {
  return _mm_round_sd(a, b, _MM_FROUND_CEIL);
}
static __inline__ __attribute__((always_inline)) __m128d _mm_floor_sd(__m128d a, __m128d b) {
  return _mm_round_sd(a, b, _MM_FROUND_FLOOR);
}
static __inline__ __attribute__((always_inline)) __m128 _mm_blendv_ps(__m128 a, __m128 b, __m128 mask) {
  return __builtin_keel_sse41_blendvps128(a, b, mask);
}
static __inline__ __attribute__((always_inline)) __m128d _mm_blendv_pd(__m128d a, __m128d b, __m128d mask) {
  return __builtin_keel_sse41_blendvpd128(a, b, mask);
}

#include <__keel_sse41_widen.h>
#include <__keel_sse41_lane_ops.h>

#include <__keel_sse41_queries.h>

#include <__keel_sse41_state.h>

#include <__keel_sse42_strings.h>

#include <crc32intrin.h>

#include <__keel_sse41_convenience.h>

#endif /* _KEEL_SMMINTRIN_H */
