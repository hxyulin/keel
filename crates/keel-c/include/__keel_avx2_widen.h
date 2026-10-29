/* Integer promotions select the low input lanes; unused input lanes are ignored. */
#ifndef _KEEL_AVX2_WIDEN_H
#define _KEEL_AVX2_WIDEN_H

static __inline__ __attribute__((always_inline)) __m256i _mm256_cvtepi8_epi16(__m128i a) {
  __keel_i8_128 x = (__keel_i8_128)a;
  return (__m256i)(__keel_i16_256){(short)x[0], (short)x[1], (short)x[2], (short)x[3], (short)x[4], (short)x[5], (short)x[6], (short)x[7], (short)x[8], (short)x[9], (short)x[10], (short)x[11], (short)x[12], (short)x[13], (short)x[14], (short)x[15]};
}
static __inline__ __attribute__((always_inline)) __m256i _mm256_cvtepi8_epi32(__m128i a) {
  __keel_i8_128 x = (__keel_i8_128)a;
  return (__m256i)(__keel_i32_256){(int)x[0], (int)x[1], (int)x[2], (int)x[3], (int)x[4], (int)x[5], (int)x[6], (int)x[7]};
}
static __inline__ __attribute__((always_inline)) __m256i _mm256_cvtepi8_epi64(__m128i a) {
  __keel_i8_128 x = (__keel_i8_128)a;
  return (__m256i)(__keel_i64_256){(long long)x[0], (long long)x[1], (long long)x[2], (long long)x[3]};
}
static __inline__ __attribute__((always_inline)) __m256i _mm256_cvtepi16_epi32(__m128i a) {
  __keel_i16_128 x = (__keel_i16_128)a;
  return (__m256i)(__keel_i32_256){(int)x[0], (int)x[1], (int)x[2], (int)x[3], (int)x[4], (int)x[5], (int)x[6], (int)x[7]};
}
static __inline__ __attribute__((always_inline)) __m256i _mm256_cvtepi16_epi64(__m128i a) {
  __keel_i16_128 x = (__keel_i16_128)a;
  return (__m256i)(__keel_i64_256){(long long)x[0], (long long)x[1], (long long)x[2], (long long)x[3]};
}
static __inline__ __attribute__((always_inline)) __m256i _mm256_cvtepi32_epi64(__m128i a) {
  __keel_i32_128 x = (__keel_i32_128)a;
  return (__m256i)(__keel_i64_256){(long long)x[0], (long long)x[1], (long long)x[2], (long long)x[3]};
}
static __inline__ __attribute__((always_inline)) __m256i _mm256_cvtepu8_epi16(__m128i a) {
  __keel_u8_128 x = (__keel_u8_128)a;
  return (__m256i)(__keel_u16_256){(unsigned short)x[0], (unsigned short)x[1], (unsigned short)x[2], (unsigned short)x[3], (unsigned short)x[4], (unsigned short)x[5], (unsigned short)x[6], (unsigned short)x[7], (unsigned short)x[8], (unsigned short)x[9], (unsigned short)x[10], (unsigned short)x[11], (unsigned short)x[12], (unsigned short)x[13], (unsigned short)x[14], (unsigned short)x[15]};
}
static __inline__ __attribute__((always_inline)) __m256i _mm256_cvtepu8_epi32(__m128i a) {
  __keel_u8_128 x = (__keel_u8_128)a;
  return (__m256i)(__keel_u32_256){(unsigned int)x[0], (unsigned int)x[1], (unsigned int)x[2], (unsigned int)x[3], (unsigned int)x[4], (unsigned int)x[5], (unsigned int)x[6], (unsigned int)x[7]};
}
static __inline__ __attribute__((always_inline)) __m256i _mm256_cvtepu8_epi64(__m128i a) {
  __keel_u8_128 x = (__keel_u8_128)a;
  return (__m256i)(__keel_u64_256){(unsigned long long)x[0], (unsigned long long)x[1], (unsigned long long)x[2], (unsigned long long)x[3]};
}
static __inline__ __attribute__((always_inline)) __m256i _mm256_cvtepu16_epi32(__m128i a) {
  __keel_u16_128 x = (__keel_u16_128)a;
  return (__m256i)(__keel_u32_256){(unsigned int)x[0], (unsigned int)x[1], (unsigned int)x[2], (unsigned int)x[3], (unsigned int)x[4], (unsigned int)x[5], (unsigned int)x[6], (unsigned int)x[7]};
}
static __inline__ __attribute__((always_inline)) __m256i _mm256_cvtepu16_epi64(__m128i a) {
  __keel_u16_128 x = (__keel_u16_128)a;
  return (__m256i)(__keel_u64_256){(unsigned long long)x[0], (unsigned long long)x[1], (unsigned long long)x[2], (unsigned long long)x[3]};
}
static __inline__ __attribute__((always_inline)) __m256i _mm256_cvtepu32_epi64(__m128i a) {
  __keel_u32_128 x = (__keel_u32_128)a;
  return (__m256i)(__keel_u64_256){(unsigned long long)x[0], (unsigned long long)x[1], (unsigned long long)x[2], (unsigned long long)x[3]};
}

#endif /* _KEEL_AVX2_WIDEN_H */
