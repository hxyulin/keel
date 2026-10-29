/* Exact lane rearrangements; generic lowering retains the header fallback
 * policy. */
#ifndef __KEEL_SSE2_LANE_OPS_H
#define __KEEL_SSE2_LANE_OPS_H

static __inline__ __attribute__((always_inline)) __m128i __keel_mm_slli_si128(__m128i a, unsigned n) {
  __keel_u8_128 x = (__keel_u8_128)a, r = {0};
  for (unsigned h = 0; h < 16; h += 16)
    for (unsigned j = 0; j < 16; ++j)
      if (n < 16 && j >= n)
        r[h + j] = x[h + j - n];
  return (__m128i)r;
}

#define _mm_slli_si128(a, i)                                                   \
  __extension__({                                                              \
    _Static_assert(                                                            \
        ((i) | 0) >= 0 && ((i) | 0) <= 255,                                    \
        "SIMD lane control must be an integer constant expression in 0..255"); \
    __keel_mm_slli_si128((a), (i));                                            \
  })

#define _mm_bslli_si128(a, i) _mm_slli_si128((a), (i))

static __inline__ __attribute__((always_inline)) __m128i __keel_mm_srli_si128(__m128i a, unsigned n) {
  __keel_u8_128 x = (__keel_u8_128)a, r = {0};
  for (unsigned h = 0; h < 16; h += 16)
    for (unsigned j = 0; j < 16; ++j)
      if (n < 16 && j + n < 16)
        r[h + j] = x[h + j + n];
  return (__m128i)r;
}

#define _mm_srli_si128(a, i)                                                   \
  __extension__({                                                              \
    _Static_assert(                                                            \
        ((i) | 0) >= 0 && ((i) | 0) <= 255,                                    \
        "SIMD lane control must be an integer constant expression in 0..255"); \
    __keel_mm_srli_si128((a), (i));                                            \
  })

#define _mm_bsrli_si128(a, i) _mm_srli_si128((a), (i))

#define _mm_shufflelo_epi16(a, i)                                              \
  __extension__({                                                              \
    _Static_assert(                                                            \
        ((i) | 0) >= 0 && ((i) | 0) <= 255,                                    \
        "SIMD lane control must be an integer constant expression in 0..255"); \
    __keel_i16_128 __keel_lane_a = (__keel_i16_128)(a);                        \
    (__m128i) __builtin_shufflevector(                                         \
        __keel_lane_a, __keel_lane_a, 0 + (((i) >> 0) & 3),                    \
        0 + (((i) >> 2) & 3), 0 + (((i) >> 4) & 3), 0 + (((i) >> 6) & 3), 4,   \
        5, 6, 7);                                                              \
  })

#define _mm_shufflehi_epi16(a, i)                                              \
  __extension__({                                                              \
    _Static_assert(                                                            \
        ((i) | 0) >= 0 && ((i) | 0) <= 255,                                    \
        "SIMD lane control must be an integer constant expression in 0..255"); \
    __keel_i16_128 __keel_lane_a = (__keel_i16_128)(a);                        \
    (__m128i) __builtin_shufflevector(                                         \
        __keel_lane_a, __keel_lane_a, 0, 1, 2, 3, 4 + (((i) >> 0) & 3),        \
        4 + (((i) >> 2) & 3), 4 + (((i) >> 4) & 3), 4 + (((i) >> 6) & 3));     \
  })

#endif /* __KEEL_SSE2_LANE_OPS_H */
