/* Exact lane construction; included after the owning vector types. */
#ifndef _KEEL_AVX2_BROADCAST_H
#define _KEEL_AVX2_BROADCAST_H
static __inline__ __attribute__((always_inline)) __m128 _mm_broadcastss_ps(__m128 a) {
  return (__m128){a[0], a[0], a[0], a[0]};
}
static __inline__ __attribute__((always_inline)) __m128d _mm_broadcastsd_pd(__m128d a) {
  return (__m128d){a[0], a[0]};
}
static __inline__ __attribute__((always_inline)) __m128i _mm_broadcastb_epi8(__m128i a) {
  char x = ((__keel_i8_128)a)[0]; return (__m128i)(__keel_i8_128){x, x, x, x, x, x, x, x, x, x, x, x, x, x, x, x};
}
static __inline__ __attribute__((always_inline)) __m128i _mm_broadcastw_epi16(__m128i a) {
  short x = ((__keel_i16_128)a)[0]; return (__m128i)(__keel_i16_128){x, x, x, x, x, x, x, x};
}
static __inline__ __attribute__((always_inline)) __m128i _mm_broadcastd_epi32(__m128i a) {
  int x = ((__keel_i32_128)a)[0]; return (__m128i)(__keel_i32_128){x, x, x, x};
}
static __inline__ __attribute__((always_inline)) __m128i _mm_broadcastq_epi64(__m128i a) {
  long long x = ((__keel_i64_128)a)[0]; return (__m128i)(__keel_i64_128){x, x};
}
static __inline__ __attribute__((always_inline)) __m256 _mm256_broadcastss_ps(__m128 a) {
  return (__m256){a[0], a[0], a[0], a[0], a[0], a[0], a[0], a[0]};
}
static __inline__ __attribute__((always_inline)) __m256d _mm256_broadcastsd_pd(__m128d a) {
  return (__m256d){a[0], a[0], a[0], a[0]};
}
static __inline__ __attribute__((always_inline)) __m256i _mm256_broadcastb_epi8(__m128i a) {
  char x = ((__keel_i8_128)a)[0]; return (__m256i)(__keel_i8_256){x, x, x, x, x, x, x, x, x, x, x, x, x, x, x, x, x, x, x, x, x, x, x, x, x, x, x, x, x, x, x, x};
}
static __inline__ __attribute__((always_inline)) __m256i _mm256_broadcastw_epi16(__m128i a) {
  short x = ((__keel_i16_128)a)[0]; return (__m256i)(__keel_i16_256){x, x, x, x, x, x, x, x, x, x, x, x, x, x, x, x};
}
static __inline__ __attribute__((always_inline)) __m256i _mm256_broadcastd_epi32(__m128i a) {
  int x = ((__keel_i32_128)a)[0]; return (__m256i)(__keel_i32_256){x, x, x, x, x, x, x, x};
}
static __inline__ __attribute__((always_inline)) __m256i _mm256_broadcastq_epi64(__m128i a) {
  long long x = ((__keel_i64_128)a)[0]; return (__m256i)(__keel_i64_256){x, x, x, x};
}
static __inline__ __attribute__((always_inline)) __m256i _mm256_broadcastsi128_si256(__m128i a) {
  return (__m256i){a[0], a[1], a[0], a[1]};
}
#define _mm_broadcastsi128_si256(a) _mm256_broadcastsi128_si256(a)
#endif /* _KEEL_AVX2_BROADCAST_H */
