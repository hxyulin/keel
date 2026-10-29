/* Exact lane construction; included after the owning vector types. */
#ifndef _KEEL_SSE2_CONSTRUCT_H
#define _KEEL_SSE2_CONSTRUCT_H
static __inline__ __attribute__((always_inline)) __m128d _mm_setr_pd(double x0, double x1) {
  return (__m128d){x0, x1};
}
static __inline__ __attribute__((always_inline)) __m128i _mm_setr_epi8(char x0, char x1, char x2, char x3, char x4, char x5, char x6, char x7, char x8, char x9, char x10, char x11, char x12, char x13, char x14, char x15) {
  return (__m128i)(__keel_i8_128){x0, x1, x2, x3, x4, x5, x6, x7, x8, x9, x10, x11, x12, x13, x14, x15};
}
static __inline__ __attribute__((always_inline)) __m128i _mm_set_epi8(char x15, char x14, char x13, char x12, char x11, char x10, char x9, char x8, char x7, char x6, char x5, char x4, char x3, char x2, char x1, char x0) {
  return (__m128i)(__keel_i8_128){x0, x1, x2, x3, x4, x5, x6, x7, x8, x9, x10, x11, x12, x13, x14, x15};
}
static __inline__ __attribute__((always_inline)) __m128i _mm_setr_epi16(short x0, short x1, short x2, short x3, short x4, short x5, short x6, short x7) {
  return (__m128i)(__keel_i16_128){x0, x1, x2, x3, x4, x5, x6, x7};
}
static __inline__ __attribute__((always_inline)) __m128i _mm_set_epi16(short x7, short x6, short x5, short x4, short x3, short x2, short x1, short x0) {
  return (__m128i)(__keel_i16_128){x0, x1, x2, x3, x4, x5, x6, x7};
}
static __inline__ __attribute__((always_inline)) __m128i _mm_setr_epi32(int x0, int x1, int x2, int x3) {
  return (__m128i)(__keel_i32_128){x0, x1, x2, x3};
}
#endif /* _KEEL_SSE2_CONSTRUCT_H */
