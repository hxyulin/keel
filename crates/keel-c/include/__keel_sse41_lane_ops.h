/* Exact lane rearrangements; generic lowering retains the header fallback
 * policy. */
#ifndef __KEEL_SSE41_LANE_OPS_H
#define __KEEL_SSE41_LANE_OPS_H

#define _mm_blend_ps(a, b, i)                                                  \
  __extension__({                                                              \
    _Static_assert(                                                            \
        ((i) | 0) >= 0 && ((i) | 0) <= 15,                                     \
        "SIMD lane control must be an integer constant expression in 0..15");  \
    (__m128) __builtin_shufflevector(                                          \
        (a), (b), 0 + 4 * (((i) >> 0) & 1), 1 + 4 * (((i) >> 1) & 1),          \
        2 + 4 * (((i) >> 2) & 1), 3 + 4 * (((i) >> 3) & 1));                   \
  })

#define _mm_blend_pd(a, b, i)                                                  \
  __extension__({                                                              \
    _Static_assert(                                                            \
        ((i) | 0) >= 0 && ((i) | 0) <= 3,                                      \
        "SIMD lane control must be an integer constant expression in 0..3");   \
    (__m128d) __builtin_shufflevector((a), (b), 0 + 2 * (((i) >> 0) & 1),      \
                                      1 + 2 * (((i) >> 1) & 1));               \
  })

#define _mm_blend_epi16(a, b, i)                                               \
  __extension__({                                                              \
    _Static_assert(                                                            \
        ((i) | 0) >= 0 && ((i) | 0) <= 255,                                    \
        "SIMD lane control must be an integer constant expression in 0..255"); \
    (__m128i) __builtin_shufflevector(                                         \
        (__keel_i16_128)(a), (__keel_i16_128)(b), 0 + 8 * (((i) >> 0) & 1),    \
        1 + 8 * (((i) >> 1) & 1), 2 + 8 * (((i) >> 2) & 1),                    \
        3 + 8 * (((i) >> 3) & 1), 4 + 8 * (((i) >> 4) & 1),                    \
        5 + 8 * (((i) >> 5) & 1), 6 + 8 * (((i) >> 6) & 1),                    \
        7 + 8 * (((i) >> 7) & 1));                                             \
  })

#endif /* __KEEL_SSE41_LANE_OPS_H */
