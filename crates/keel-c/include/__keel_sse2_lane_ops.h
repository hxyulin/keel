/* Exact lane rearrangements; generic lowering retains the header fallback
 * policy. */
#ifndef __KEEL_SSE2_LANE_OPS_H
#define __KEEL_SSE2_LANE_OPS_H

/* Byte shifts and alignr are constant shufflevector selections. Each
 * *_LANE macro lists the 16 indices of lane h (byte offset) in a len-byte
 * vector. Zero bytes come from `x & 0`, a register zero; a `{0}` literal is
 * built in memory. Index expressions avoid `||`, which lowers to branches. */
#define __KEEL_BSLL_LANE(n, h, len)                                                                             \
  __KEEL_BSLL(n, h, 0, len), __KEEL_BSLL(n, h, 1, len), __KEEL_BSLL(n, h, 2, len), __KEEL_BSLL(n, h, 3, len),   \
  __KEEL_BSLL(n, h, 4, len), __KEEL_BSLL(n, h, 5, len), __KEEL_BSLL(n, h, 6, len), __KEEL_BSLL(n, h, 7, len),   \
  __KEEL_BSLL(n, h, 8, len), __KEEL_BSLL(n, h, 9, len), __KEEL_BSLL(n, h, 10, len), __KEEL_BSLL(n, h, 11, len), \
  __KEEL_BSLL(n, h, 12, len), __KEEL_BSLL(n, h, 13, len), __KEEL_BSLL(n, h, 14, len), __KEEL_BSLL(n, h, 15, len)
#define __KEEL_BSRL_LANE(n, h, len)                                                                             \
  __KEEL_BSRL(n, h, 0, len), __KEEL_BSRL(n, h, 1, len), __KEEL_BSRL(n, h, 2, len), __KEEL_BSRL(n, h, 3, len),   \
  __KEEL_BSRL(n, h, 4, len), __KEEL_BSRL(n, h, 5, len), __KEEL_BSRL(n, h, 6, len), __KEEL_BSRL(n, h, 7, len),   \
  __KEEL_BSRL(n, h, 8, len), __KEEL_BSRL(n, h, 9, len), __KEEL_BSRL(n, h, 10, len), __KEEL_BSRL(n, h, 11, len), \
  __KEEL_BSRL(n, h, 12, len), __KEEL_BSRL(n, h, 13, len), __KEEL_BSRL(n, h, 14, len), __KEEL_BSRL(n, h, 15, len)
#define __KEEL_ALIGNR_LANE(n, h, len)                                                                                   \
  __KEEL_ALIGNR(n, h, 0, len), __KEEL_ALIGNR(n, h, 1, len), __KEEL_ALIGNR(n, h, 2, len), __KEEL_ALIGNR(n, h, 3, len),   \
  __KEEL_ALIGNR(n, h, 4, len), __KEEL_ALIGNR(n, h, 5, len), __KEEL_ALIGNR(n, h, 6, len), __KEEL_ALIGNR(n, h, 7, len),   \
  __KEEL_ALIGNR(n, h, 8, len), __KEEL_ALIGNR(n, h, 9, len), __KEEL_ALIGNR(n, h, 10, len), __KEEL_ALIGNR(n, h, 11, len), \
  __KEEL_ALIGNR(n, h, 12, len), __KEEL_ALIGNR(n, h, 13, len), __KEEL_ALIGNR(n, h, 14, len), __KEEL_ALIGNR(n, h, 15, len)
/* Shifted left: bytes below n come from the zero operand. */
#define __KEEL_BSLL(n, h, j, len) \
  ((n) > 15 ? (h) + (j) : (j) < (n) ? (h) + (j) : (len) + (h) + (j) - (n))
/* Shifted right: bytes from n + j past the lane come from the zero operand. */
#define __KEEL_BSRL(n, h, j, len) \
  ((n) + (j) > 15 ? (len) + (h) + (j) : (h) + (j) + (n))
/* The lane pair (first, second) shifted right by n mod 16 bytes. */
#define __KEEL_ALIGNR(n, h, j, len) \
  (((n) & 15) + (j) < 16 ? (h) + ((n) & 15) + (j) : (len) + (h) + ((n) & 15) + (j) - 16)

#define _mm_slli_si128(a, i)                                                   \
  __extension__({                                                              \
    _Static_assert(                                                            \
        ((i) | 0) >= 0 && ((i) | 0) <= 255,                                    \
        "SIMD lane control must be an integer constant expression in 0..255"); \
    __keel_u8_128 __keel_a = (__keel_u8_128)(a);                               \
    (__m128i) __builtin_shufflevector(                                         \
        __keel_a & 0, __keel_a,                                                \
        __KEEL_BSLL_LANE((i), 0, 16));                                         \
  })

#define _mm_bslli_si128(a, i) _mm_slli_si128((a), (i))

#define _mm_srli_si128(a, i)                                                   \
  __extension__({                                                              \
    _Static_assert(                                                            \
        ((i) | 0) >= 0 && ((i) | 0) <= 255,                                    \
        "SIMD lane control must be an integer constant expression in 0..255"); \
    __keel_u8_128 __keel_a = (__keel_u8_128)(a);                               \
    (__m128i) __builtin_shufflevector(                                         \
        __keel_a, __keel_a & 0,                                                \
        __KEEL_BSRL_LANE((i), 0, 16));                                         \
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
