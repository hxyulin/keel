/* Integer promotions select the low input lanes; unused input lanes are ignored. */
#ifndef _KEEL_SSE41_WIDEN_H
#define _KEEL_SSE41_WIDEN_H

static __inline__ __attribute__((always_inline)) __m128i _mm_cvtepi8_epi16(__m128i a) {
  __keel_i8_128 x = (__keel_i8_128)a;
  return (__m128i)(__keel_i16_128){(short)x[0], (short)x[1], (short)x[2], (short)x[3], (short)x[4], (short)x[5], (short)x[6], (short)x[7]};
}
static __inline__ __attribute__((always_inline)) __m128i _mm_cvtepi8_epi32(__m128i a) {
  __keel_i8_128 x = (__keel_i8_128)a;
  return (__m128i)(__keel_i32_128){(int)x[0], (int)x[1], (int)x[2], (int)x[3]};
}
static __inline__ __attribute__((always_inline)) __m128i _mm_cvtepi8_epi64(__m128i a) {
  __keel_i8_128 x = (__keel_i8_128)a;
  return (__m128i)(__keel_i64_128){(long long)x[0], (long long)x[1]};
}
static __inline__ __attribute__((always_inline)) __m128i _mm_cvtepi16_epi32(__m128i a) {
  __keel_i16_128 x = (__keel_i16_128)a;
  return (__m128i)(__keel_i32_128){(int)x[0], (int)x[1], (int)x[2], (int)x[3]};
}
static __inline__ __attribute__((always_inline)) __m128i _mm_cvtepi16_epi64(__m128i a) {
  __keel_i16_128 x = (__keel_i16_128)a;
  return (__m128i)(__keel_i64_128){(long long)x[0], (long long)x[1]};
}
static __inline__ __attribute__((always_inline)) __m128i _mm_cvtepi32_epi64(__m128i a) {
  __keel_i32_128 x = (__keel_i32_128)a;
  return (__m128i)(__keel_i64_128){(long long)x[0], (long long)x[1]};
}
static __inline__ __attribute__((always_inline)) __m128i _mm_cvtepu8_epi16(__m128i a) {
  __keel_u8_128 x = (__keel_u8_128)a;
  return (__m128i)(__keel_u16_128){(unsigned short)x[0], (unsigned short)x[1], (unsigned short)x[2], (unsigned short)x[3], (unsigned short)x[4], (unsigned short)x[5], (unsigned short)x[6], (unsigned short)x[7]};
}
static __inline__ __attribute__((always_inline)) __m128i _mm_cvtepu8_epi32(__m128i a) {
  __keel_u8_128 x = (__keel_u8_128)a;
  return (__m128i)(__keel_u32_128){(unsigned int)x[0], (unsigned int)x[1], (unsigned int)x[2], (unsigned int)x[3]};
}
static __inline__ __attribute__((always_inline)) __m128i _mm_cvtepu8_epi64(__m128i a) {
  __keel_u8_128 x = (__keel_u8_128)a;
  return (__m128i)(__keel_u64_128){(unsigned long long)x[0], (unsigned long long)x[1]};
}
static __inline__ __attribute__((always_inline)) __m128i _mm_cvtepu16_epi32(__m128i a) {
  __keel_u16_128 x = (__keel_u16_128)a;
  return (__m128i)(__keel_u32_128){(unsigned int)x[0], (unsigned int)x[1], (unsigned int)x[2], (unsigned int)x[3]};
}
static __inline__ __attribute__((always_inline)) __m128i _mm_cvtepu16_epi64(__m128i a) {
  __keel_u16_128 x = (__keel_u16_128)a;
  return (__m128i)(__keel_u64_128){(unsigned long long)x[0], (unsigned long long)x[1]};
}
static __inline__ __attribute__((always_inline)) __m128i _mm_cvtepu32_epi64(__m128i a) {
  __keel_u32_128 x = (__keel_u32_128)a;
  return (__m128i)(__keel_u64_128){(unsigned long long)x[0], (unsigned long long)x[1]};
}

#endif /* _KEEL_SSE41_WIDEN_H */
