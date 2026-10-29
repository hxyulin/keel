#ifndef _KEEL_SSE3_CONVENIENCE_H
#define _KEEL_SSE3_CONVENIENCE_H
static __inline__ __attribute__((always_inline)) __m128i _mm_lddqu_si128(const __m128i_u *p) { __m128i r;__builtin_memcpy(&r,p,16);return r; }
#endif /* _KEEL_SSE3_CONVENIENCE_H */
