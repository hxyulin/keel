/* Bounded Keel SSSE3 intrinsic subset. */
#ifndef _KEEL_TMMINTRIN_H
#define _KEEL_TMMINTRIN_H
#ifndef __x86_64__
#error "Keel x86 intrinsic headers require x86-64"
#endif
#include <pmmintrin.h>

/* Unsigned negation retains the minimum signed lane bit pattern. */
static __inline__ __attribute__((always_inline)) __m128i _mm_abs_epi8(__m128i a) {
  __keel_u8_128 x = (__keel_u8_128)a;
  __keel_u8_128 negative = (__keel_u8_128)((__keel_i8_128)a < (__keel_i8_128){0});
  return (__m128i)((x & ~negative) | (((__keel_u8_128){0} - x) & negative));
}
static __inline__ __attribute__((always_inline)) __m128i _mm_sign_epi8(__m128i a, __m128i b) {
  __keel_u8_128 x = (__keel_u8_128)a;
  __keel_i8_128 selector = (__keel_i8_128)b;
  __keel_u8_128 positive = (__keel_u8_128)(selector > (__keel_i8_128){0});
  __keel_u8_128 negative = (__keel_u8_128)(selector < (__keel_i8_128){0});
  return (__m128i)((x & positive) | (((__keel_u8_128){0} - x) & negative));
}
static __inline__ __attribute__((always_inline)) __m128i _mm_abs_epi16(__m128i a) {
  __keel_u16_128 x = (__keel_u16_128)a;
  __keel_u16_128 negative = (__keel_u16_128)((__keel_i16_128)a < (__keel_i16_128){0});
  return (__m128i)((x & ~negative) | (((__keel_u16_128){0} - x) & negative));
}
static __inline__ __attribute__((always_inline)) __m128i _mm_sign_epi16(__m128i a, __m128i b) {
  __keel_u16_128 x = (__keel_u16_128)a;
  __keel_i16_128 selector = (__keel_i16_128)b;
  __keel_u16_128 positive = (__keel_u16_128)(selector > (__keel_i16_128){0});
  __keel_u16_128 negative = (__keel_u16_128)(selector < (__keel_i16_128){0});
  return (__m128i)((x & positive) | (((__keel_u16_128){0} - x) & negative));
}
static __inline__ __attribute__((always_inline)) __m128i _mm_abs_epi32(__m128i a) {
  __keel_u32_128 x = (__keel_u32_128)a;
  __keel_u32_128 negative = (__keel_u32_128)((__keel_i32_128)a < (__keel_i32_128){0});
  return (__m128i)((x & ~negative) | (((__keel_u32_128){0} - x) & negative));
}
static __inline__ __attribute__((always_inline)) __m128i _mm_sign_epi32(__m128i a, __m128i b) {
  __keel_u32_128 x = (__keel_u32_128)a;
  __keel_i32_128 selector = (__keel_i32_128)b;
  __keel_u32_128 positive = (__keel_u32_128)(selector > (__keel_i32_128){0});
  __keel_u32_128 negative = (__keel_u32_128)(selector < (__keel_i32_128){0});
  return (__m128i)((x & positive) | (((__keel_u32_128){0} - x) & negative));
}
static __inline__ __attribute__((always_inline)) __m128i _mm_maddubs_epi16(__m128i a, __m128i b) {
  return (__m128i)__builtin_keel_ssse3_pmaddubsw128((__keel_i8_128)a, (__keel_i8_128)b);
}
static __inline__ __attribute__((always_inline)) __m128i _mm_mulhrs_epi16(__m128i a, __m128i b) {
  return (__m128i)__builtin_keel_ssse3_pmulhrsw128((__keel_i16_128)a, (__keel_i16_128)b);
}
static __inline__ __attribute__((always_inline)) __m128i _mm_shuffle_epi8(__m128i a, __m128i b) {
  return (__m128i)__builtin_keel_ssse3_pshufb128((__keel_i8_128)a, (__keel_i8_128)b);
}

static __inline__ __attribute__((always_inline)) __m128i _mm_hadd_epi16(__m128i a, __m128i b) {
  return (__m128i)__builtin_keel_ssse3_phaddw128((__keel_i16_128)a, (__keel_i16_128)b);
}
static __inline__ __attribute__((always_inline)) __m128i _mm_hadd_epi32(__m128i a, __m128i b) {
  return (__m128i)__builtin_keel_ssse3_phaddd128((__keel_i32_128)a, (__keel_i32_128)b);
}
static __inline__ __attribute__((always_inline)) __m128i _mm_hadds_epi16(__m128i a, __m128i b) {
  return (__m128i)__builtin_keel_ssse3_phaddsw128((__keel_i16_128)a, (__keel_i16_128)b);
}
static __inline__ __attribute__((always_inline)) __m128i _mm_hsub_epi16(__m128i a, __m128i b) {
  return (__m128i)__builtin_keel_ssse3_phsubw128((__keel_i16_128)a, (__keel_i16_128)b);
}
static __inline__ __attribute__((always_inline)) __m128i _mm_hsub_epi32(__m128i a, __m128i b) {
  return (__m128i)__builtin_keel_ssse3_phsubd128((__keel_i32_128)a, (__keel_i32_128)b);
}
static __inline__ __attribute__((always_inline)) __m128i _mm_hsubs_epi16(__m128i a, __m128i b) {
  return (__m128i)__builtin_keel_ssse3_phsubsw128((__keel_i16_128)a, (__keel_i16_128)b);
}

#include <__keel_ssse3_lane_ops.h>

#endif /* _KEEL_TMMINTRIN_H */
