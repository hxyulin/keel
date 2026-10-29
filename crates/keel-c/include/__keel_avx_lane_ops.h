/* Exact lane rearrangements; generic lowering retains the header fallback
 * policy. */
#ifndef __KEEL_AVX_LANE_OPS_H
#define __KEEL_AVX_LANE_OPS_H

#define _mm256_blend_ps(a, b, i)                                               \
  __extension__({                                                              \
    _Static_assert(                                                            \
        ((i) | 0) >= 0 && ((i) | 0) <= 255,                                    \
        "SIMD lane control must be an integer constant expression in 0..255"); \
    (__m256) __builtin_shufflevector(                                          \
        (a), (b), 0 + 8 * (((i) >> 0) & 1), 1 + 8 * (((i) >> 1) & 1),          \
        2 + 8 * (((i) >> 2) & 1), 3 + 8 * (((i) >> 3) & 1),                    \
        4 + 8 * (((i) >> 4) & 1), 5 + 8 * (((i) >> 5) & 1),                    \
        6 + 8 * (((i) >> 6) & 1), 7 + 8 * (((i) >> 7) & 1));                   \
  })

#define _mm256_blend_pd(a, b, i)                                               \
  __extension__({                                                              \
    _Static_assert(                                                            \
        ((i) | 0) >= 0 && ((i) | 0) <= 15,                                     \
        "SIMD lane control must be an integer constant expression in 0..15");  \
    (__m256d) __builtin_shufflevector(                                         \
        (a), (b), 0 + 4 * (((i) >> 0) & 1), 1 + 4 * (((i) >> 1) & 1),          \
        2 + 4 * (((i) >> 2) & 1), 3 + 4 * (((i) >> 3) & 1));                   \
  })

#define _mm256_shuffle_pd(a, b, i)                                             \
  __extension__({                                                              \
    _Static_assert(                                                            \
        ((i) | 0) >= 0 && ((i) | 0) <= 15,                                     \
        "SIMD lane control must be an integer constant expression in 0..15");  \
    __builtin_shufflevector((a), (b), (i) & 1, 4 + (((i) >> 1) & 1),           \
                            2 + (((i) >> 2) & 1), 6 + (((i) >> 3) & 1));       \
  })

#define _mm_permute_ps(a, i)                                                   \
  __extension__({                                                              \
    _Static_assert(                                                            \
        ((i) | 0) >= 0 && ((i) | 0) <= 255,                                    \
        "SIMD lane control must be an integer constant expression in 0..255"); \
    __m128 __keel_lane_a = (a);                                                \
    __builtin_shufflevector(__keel_lane_a, __keel_lane_a,                      \
                            0 + (((i) >> 0) & 3), 0 + (((i) >> 2) & 3),        \
                            0 + (((i) >> 4) & 3), 0 + (((i) >> 6) & 3));       \
  })

typedef int __keel_permute_i32_128 __attribute__((vector_size(16)));

static __inline__ __attribute__((always_inline)) __m128 _mm_permutevar_ps(__m128 a, __m128i control) {
  __keel_permute_i32_128 c = (__keel_permute_i32_128)control;
  __m128 r;
  for (unsigned j = 0; j < 4; ++j)
    r[j] = a[(j / 4) * 4 + ((unsigned)c[j] & 3)];
  return r;
}

#define _mm_permute_pd(a, i)                                                   \
  __extension__({                                                              \
    _Static_assert(                                                            \
        ((i) | 0) >= 0 && ((i) | 0) <= 3,                                      \
        "SIMD lane control must be an integer constant expression in 0..3");   \
    __m128d __keel_lane_a = (a);                                               \
    __builtin_shufflevector(__keel_lane_a, __keel_lane_a,                      \
                            0 + (((i) >> 0) & 1), 0 + (((i) >> 1) & 1));       \
  })

typedef long long __keel_permute_i64_128 __attribute__((vector_size(16)));

static __inline__ __attribute__((always_inline)) __m128d _mm_permutevar_pd(__m128d a, __m128i control) {
  __keel_permute_i64_128 c = (__keel_permute_i64_128)control;
  __m128d r;
  for (unsigned j = 0; j < 2; ++j)
    r[j] = a[(j / 2) * 2 + (((unsigned long long)c[j] >> 1) & 1)];
  return r;
}

#define _mm256_permute_ps(a, i)                                                \
  __extension__({                                                              \
    _Static_assert(                                                            \
        ((i) | 0) >= 0 && ((i) | 0) <= 255,                                    \
        "SIMD lane control must be an integer constant expression in 0..255"); \
    __m256 __keel_lane_a = (a);                                                \
    __builtin_shufflevector(__keel_lane_a, __keel_lane_a,                      \
                            0 + (((i) >> 0) & 3), 0 + (((i) >> 2) & 3),        \
                            0 + (((i) >> 4) & 3), 0 + (((i) >> 6) & 3),        \
                            4 + (((i) >> 0) & 3), 4 + (((i) >> 2) & 3),        \
                            4 + (((i) >> 4) & 3), 4 + (((i) >> 6) & 3));       \
  })


static __inline__ __attribute__((always_inline)) __m256 _mm256_permutevar_ps(__m256 a, __m256i control) {
  __keel_i32_256 c = (__keel_i32_256)control;
  __m256 r;
  for (unsigned j = 0; j < 8; ++j)
    r[j] = a[(j / 4) * 4 + ((unsigned)c[j] & 3)];
  return r;
}

#define _mm256_permute_pd(a, i)                                                \
  __extension__({                                                              \
    _Static_assert(                                                            \
        ((i) | 0) >= 0 && ((i) | 0) <= 15,                                     \
        "SIMD lane control must be an integer constant expression in 0..15");  \
    __m256d __keel_lane_a = (a);                                               \
    __builtin_shufflevector(__keel_lane_a, __keel_lane_a,                      \
                            0 + (((i) >> 0) & 1), 0 + (((i) >> 1) & 1),        \
                            2 + (((i) >> 2) & 1), 2 + (((i) >> 3) & 1));       \
  })


static __inline__ __attribute__((always_inline)) __m256d _mm256_permutevar_pd(__m256d a, __m256i control) {
  __keel_i64_256 c = (__keel_i64_256)control;
  __m256d r;
  for (unsigned j = 0; j < 4; ++j)
    r[j] = a[(j / 2) * 2 + (((unsigned long long)c[j] >> 1) & 1)];
  return r;
}

static __inline__ __attribute__((always_inline)) __m256i __keel_permute2f128(__m256i a, __m256i b,
                                              unsigned c) {
  __m256i r = {0};
  for (unsigned h = 0; h < 2; ++h) {
    unsigned k = (c >> (h * 4)) & 3;
    if (!((c >> (h * 4)) & 8)) {
      r[h * 2] = k < 2 ? a[k * 2] : b[(k - 2) * 2];
      r[h * 2 + 1] = k < 2 ? a[k * 2 + 1] : b[(k - 2) * 2 + 1];
    }
  }
  return r;
}

#define _mm256_permute2f128_ps(a, b, i)                                        \
  __extension__({                                                              \
    _Static_assert(                                                            \
        ((i) | 0) >= 0 && ((i) | 0) <= 255,                                    \
        "SIMD lane control must be an integer constant expression in 0..255"); \
    (__m256) __keel_permute2f128((__m256i)(a), (__m256i)(b), (i));             \
  })

#define _mm256_permute2f128_pd(a, b, i)                                        \
  __extension__({                                                              \
    _Static_assert(                                                            \
        ((i) | 0) >= 0 && ((i) | 0) <= 255,                                    \
        "SIMD lane control must be an integer constant expression in 0..255"); \
    (__m256d) __keel_permute2f128((__m256i)(a), (__m256i)(b), (i));            \
  })

#define _mm256_permute2f128_si256(a, b, i)                                     \
  __extension__({                                                              \
    _Static_assert(                                                            \
        ((i) | 0) >= 0 && ((i) | 0) <= 255,                                    \
        "SIMD lane control must be an integer constant expression in 0..255"); \
    (__m256i) __keel_permute2f128((__m256i)(a), (__m256i)(b), (i));            \
  })

#endif /* __KEEL_AVX_LANE_OPS_H */
