/* Exact lane rearrangements; generic lowering retains the header fallback
 * policy. */
#ifndef __KEEL_SSSE3_LANE_OPS_H
#define __KEEL_SSSE3_LANE_OPS_H

static __inline__ __attribute__((always_inline)) __m128i __keel_alignr_128(__m128i a, __m128i b, unsigned n) {
  __keel_u8_128 x = (__keel_u8_128)a, y = (__keel_u8_128)b, r = {0};
  for (unsigned h = 0; h < 16; h += 16)
    for (unsigned j = 0; j < 16; ++j) {
      unsigned k = n + j;
      if (k < 16)
        r[h + j] = y[h + k];
      else if (k < 32)
        r[h + j] = x[h + k - 16];
    }
  return (__m128i)r;
}

#define _mm_alignr_epi8(a, b, i)                                               \
  __extension__({                                                              \
    _Static_assert(                                                            \
        ((i) | 0) >= 0 && ((i) | 0) <= 255,                                    \
        "SIMD lane control must be an integer constant expression in 0..255"); \
    __keel_alignr_128((a), (b), (i));                                          \
  })

#endif /* __KEEL_SSSE3_LANE_OPS_H */
