/* Exact lane rearrangements; generic lowering retains the header fallback
 * policy. */
#ifndef __KEEL_AVX2_LANE_OPS_H
#define __KEEL_AVX2_LANE_OPS_H

static __inline__ __attribute__((always_inline)) __m256i __keel_mm256_slli_si256(__m256i a, unsigned n) {
  __keel_u8_256 x = (__keel_u8_256)a, r = {0};
  for (unsigned h = 0; h < 32; h += 16)
    for (unsigned j = 0; j < 16; ++j)
      if (n < 16 && j >= n)
        r[h + j] = x[h + j - n];
  return (__m256i)r;
}

#define _mm256_slli_si256(a, i)                                                \
  __extension__({                                                              \
    _Static_assert(                                                            \
        ((i) | 0) >= 0 && ((i) | 0) <= 255,                                    \
        "SIMD lane control must be an integer constant expression in 0..255"); \
    __keel_mm256_slli_si256((a), (i));                                         \
  })

#define _mm256_bslli_epi128(a, i) _mm256_slli_si256((a), (i))

static __inline__ __attribute__((always_inline)) __m256i __keel_mm256_srli_si256(__m256i a, unsigned n) {
  __keel_u8_256 x = (__keel_u8_256)a, r = {0};
  for (unsigned h = 0; h < 32; h += 16)
    for (unsigned j = 0; j < 16; ++j)
      if (n < 16 && j + n < 16)
        r[h + j] = x[h + j + n];
  return (__m256i)r;
}

#define _mm256_srli_si256(a, i)                                                \
  __extension__({                                                              \
    _Static_assert(                                                            \
        ((i) | 0) >= 0 && ((i) | 0) <= 255,                                    \
        "SIMD lane control must be an integer constant expression in 0..255"); \
    __keel_mm256_srli_si256((a), (i));                                         \
  })

#define _mm256_bsrli_epi128(a, i) _mm256_srli_si256((a), (i))

#define _mm256_shufflelo_epi16(a, i)                                           \
  __extension__({                                                              \
    _Static_assert(                                                            \
        ((i) | 0) >= 0 && ((i) | 0) <= 255,                                    \
        "SIMD lane control must be an integer constant expression in 0..255"); \
    __keel_i16_256 __keel_lane_a = (__keel_i16_256)(a);                        \
    (__m256i) __builtin_shufflevector(                                         \
        __keel_lane_a, __keel_lane_a, 0 + (((i) >> 0) & 3),                    \
        0 + (((i) >> 2) & 3), 0 + (((i) >> 4) & 3), 0 + (((i) >> 6) & 3), 4,   \
        5, 6, 7, 8 + (((i) >> 0) & 3), 8 + (((i) >> 2) & 3),                   \
        8 + (((i) >> 4) & 3), 8 + (((i) >> 6) & 3), 12, 13, 14, 15);           \
  })

#define _mm256_shufflehi_epi16(a, i)                                           \
  __extension__({                                                              \
    _Static_assert(                                                            \
        ((i) | 0) >= 0 && ((i) | 0) <= 255,                                    \
        "SIMD lane control must be an integer constant expression in 0..255"); \
    __keel_i16_256 __keel_lane_a = (__keel_i16_256)(a);                        \
    (__m256i) __builtin_shufflevector(                                         \
        __keel_lane_a, __keel_lane_a, 0, 1, 2, 3, 4 + (((i) >> 0) & 3),        \
        4 + (((i) >> 2) & 3), 4 + (((i) >> 4) & 3), 4 + (((i) >> 6) & 3), 8,   \
        9, 10, 11, 12 + (((i) >> 0) & 3), 12 + (((i) >> 2) & 3),               \
        12 + (((i) >> 4) & 3), 12 + (((i) >> 6) & 3));                         \
  })

static __inline__ __attribute__((always_inline)) __m256i __keel_alignr_256(__m256i a, __m256i b, unsigned n) {
  __keel_u8_256 x = (__keel_u8_256)a, y = (__keel_u8_256)b, r = {0};
  for (unsigned h = 0; h < 32; h += 16)
    for (unsigned j = 0; j < 16; ++j) {
      unsigned k = n + j;
      if (k < 16)
        r[h + j] = y[h + k];
      else if (k < 32)
        r[h + j] = x[h + k - 16];
    }
  return (__m256i)r;
}

#define _mm256_alignr_epi8(a, b, i)                                            \
  __extension__({                                                              \
    _Static_assert(                                                            \
        ((i) | 0) >= 0 && ((i) | 0) <= 255,                                    \
        "SIMD lane control must be an integer constant expression in 0..255"); \
    __keel_alignr_256((a), (b), (i));                                          \
  })

#define _mm_blend_epi32(a, b, i)                                               \
  __extension__({                                                              \
    _Static_assert(                                                            \
        ((i) | 0) >= 0 && ((i) | 0) <= 15,                                     \
        "SIMD lane control must be an integer constant expression in 0..15");  \
    (__m128i) __builtin_shufflevector(                                         \
        (__keel_i32_128)(a), (__keel_i32_128)(b), 0 + 4 * (((i) >> 0) & 1),    \
        1 + 4 * (((i) >> 1) & 1), 2 + 4 * (((i) >> 2) & 1),                    \
        3 + 4 * (((i) >> 3) & 1));                                             \
  })

#define _mm256_blend_epi16(a, b, i)                                            \
  __extension__({                                                              \
    _Static_assert(                                                            \
        ((i) | 0) >= 0 && ((i) | 0) <= 255,                                    \
        "SIMD lane control must be an integer constant expression in 0..255"); \
    (__m256i) __builtin_shufflevector(                                         \
        (__keel_i16_256)(a), (__keel_i16_256)(b), 0 + 16 * (((i) >> 0) & 1),   \
        1 + 16 * (((i) >> 1) & 1), 2 + 16 * (((i) >> 2) & 1),                  \
        3 + 16 * (((i) >> 3) & 1), 4 + 16 * (((i) >> 4) & 1),                  \
        5 + 16 * (((i) >> 5) & 1), 6 + 16 * (((i) >> 6) & 1),                  \
        7 + 16 * (((i) >> 7) & 1), 8 + 16 * (((i) >> 0) & 1),                  \
        9 + 16 * (((i) >> 1) & 1), 10 + 16 * (((i) >> 2) & 1),                 \
        11 + 16 * (((i) >> 3) & 1), 12 + 16 * (((i) >> 4) & 1),                \
        13 + 16 * (((i) >> 5) & 1), 14 + 16 * (((i) >> 6) & 1),                \
        15 + 16 * (((i) >> 7) & 1));                                           \
  })

#define _mm256_blend_epi32(a, b, i)                                            \
  __extension__({                                                              \
    _Static_assert(                                                            \
        ((i) | 0) >= 0 && ((i) | 0) <= 255,                                    \
        "SIMD lane control must be an integer constant expression in 0..255"); \
    (__m256i) __builtin_shufflevector(                                         \
        (__keel_i32_256)(a), (__keel_i32_256)(b), 0 + 8 * (((i) >> 0) & 1),    \
        1 + 8 * (((i) >> 1) & 1), 2 + 8 * (((i) >> 2) & 1),                    \
        3 + 8 * (((i) >> 3) & 1), 4 + 8 * (((i) >> 4) & 1),                    \
        5 + 8 * (((i) >> 5) & 1), 6 + 8 * (((i) >> 6) & 1),                    \
        7 + 8 * (((i) >> 7) & 1));                                             \
  })

#define _mm256_permute2x128_si256(a, b, i)                                     \
  _mm256_permute2f128_si256((a), (b), (i))

#define _mm256_permute4x64_pd(a, i)                                            \
  __extension__({                                                              \
    _Static_assert(                                                            \
        ((i) | 0) >= 0 && ((i) | 0) <= 255,                                    \
        "SIMD lane control must be an integer constant expression in 0..255"); \
    __m256d __keel_lane_a = (a);                                               \
    __builtin_shufflevector(__keel_lane_a, __keel_lane_a, (i) & 3,             \
                            ((i) >> 2) & 3, ((i) >> 4) & 3, ((i) >> 6) & 3);   \
  })

static __inline__ __attribute__((always_inline)) __m256i _mm256_permutevar8x32_epi32(__m256i a,
                                                      __m256i control) {
  __keel_u32_256 x = (__keel_u32_256)a, c = (__keel_u32_256)control, r;
  for (unsigned j = 0; j < 8; ++j)
    r[j] = x[c[j] & 7];
  return (__m256i)r;
}

static __inline__ __attribute__((always_inline)) __m256 _mm256_permutevar8x32_ps(__m256 a, __m256i control) {
  __keel_u32_256 x = (__keel_u32_256)a, c = (__keel_u32_256)control, r;
  for (unsigned j = 0; j < 8; ++j)
    r[j] = x[c[j] & 7];
  return (__m256)r;
}

#endif /* __KEEL_AVX2_LANE_OPS_H */
