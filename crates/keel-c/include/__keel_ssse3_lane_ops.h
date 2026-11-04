/* Exact lane rearrangements; generic lowering retains the header fallback
 * policy. */
#ifndef __KEEL_SSSE3_LANE_OPS_H
#define __KEEL_SSSE3_LANE_OPS_H

#define _mm_alignr_epi8(a, b, i)                                                \
  __extension__({                                                               \
    _Static_assert(                                                             \
        ((i) | 0) >= 0 && ((i) | 0) <= 255,                                     \
        "SIMD lane control must be an integer constant expression in 0..255");  \
    __keel_u8_128 __keel_a = (__keel_u8_128)(a), __keel_b = (__keel_u8_128)(b); \
    (__m128i) __builtin_shufflevector(                                          \
        (i) < 16 ? __keel_b : (i) < 32 ? __keel_a : __keel_a & 0,               \
        (i) < 16 ? __keel_a : __keel_a & 0,                                     \
        __KEEL_ALIGNR_LANE((i), 0, 16));                                        \
  })

#endif /* __KEEL_SSSE3_LANE_OPS_H */
