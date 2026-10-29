/* Exact lane construction; included after the owning vector types. */
#ifndef _KEEL_AVX_CONSTRUCT_H
#define _KEEL_AVX_CONSTRUCT_H
static __inline__ __attribute__((always_inline)) __m256 _mm256_setr_ps(float x0, float x1, float x2, float x3, float x4, float x5, float x6, float x7) {
  return (__m256){x0, x1, x2, x3, x4, x5, x6, x7};
}
static __inline__ __attribute__((always_inline)) __m256d _mm256_setr_pd(double x0, double x1, double x2, double x3) {
  return (__m256d){x0, x1, x2, x3};
}
static __inline__ __attribute__((always_inline)) __m256i _mm256_setr_epi8(char x0, char x1, char x2, char x3, char x4, char x5, char x6, char x7, char x8, char x9, char x10, char x11, char x12, char x13, char x14, char x15, char x16, char x17, char x18, char x19, char x20, char x21, char x22, char x23, char x24, char x25, char x26, char x27, char x28, char x29, char x30, char x31) {
  return (__m256i)(__keel_i8_256){x0, x1, x2, x3, x4, x5, x6, x7, x8, x9, x10, x11, x12, x13, x14, x15, x16, x17, x18, x19, x20, x21, x22, x23, x24, x25, x26, x27, x28, x29, x30, x31};
}
static __inline__ __attribute__((always_inline)) __m256i _mm256_set_epi8(char x31, char x30, char x29, char x28, char x27, char x26, char x25, char x24, char x23, char x22, char x21, char x20, char x19, char x18, char x17, char x16, char x15, char x14, char x13, char x12, char x11, char x10, char x9, char x8, char x7, char x6, char x5, char x4, char x3, char x2, char x1, char x0) {
  return (__m256i)(__keel_i8_256){x0, x1, x2, x3, x4, x5, x6, x7, x8, x9, x10, x11, x12, x13, x14, x15, x16, x17, x18, x19, x20, x21, x22, x23, x24, x25, x26, x27, x28, x29, x30, x31};
}
static __inline__ __attribute__((always_inline)) __m256i _mm256_setr_epi16(short x0, short x1, short x2, short x3, short x4, short x5, short x6, short x7, short x8, short x9, short x10, short x11, short x12, short x13, short x14, short x15) {
  return (__m256i)(__keel_i16_256){x0, x1, x2, x3, x4, x5, x6, x7, x8, x9, x10, x11, x12, x13, x14, x15};
}
static __inline__ __attribute__((always_inline)) __m256i _mm256_set_epi16(short x15, short x14, short x13, short x12, short x11, short x10, short x9, short x8, short x7, short x6, short x5, short x4, short x3, short x2, short x1, short x0) {
  return (__m256i)(__keel_i16_256){x0, x1, x2, x3, x4, x5, x6, x7, x8, x9, x10, x11, x12, x13, x14, x15};
}
static __inline__ __attribute__((always_inline)) __m256i _mm256_setr_epi32(int x0, int x1, int x2, int x3, int x4, int x5, int x6, int x7) {
  return (__m256i)(__keel_i32_256){x0, x1, x2, x3, x4, x5, x6, x7};
}
static __inline__ __attribute__((always_inline)) __m256i _mm256_setr_epi64x(long long x0, long long x1, long long x2, long long x3) {
  return (__m256i)(__keel_i64_256){x0, x1, x2, x3};
}
static __inline__ __attribute__((always_inline)) __m256 _mm256_movehdup_ps(__m256 a) {
  return (__m256)__builtin_shufflevector(a, a, 1, 1, 3, 3, 5, 5, 7, 7);
}
static __inline__ __attribute__((always_inline)) __m256 _mm256_moveldup_ps(__m256 a) {
  return (__m256)__builtin_shufflevector(a, a, 0, 0, 2, 2, 4, 4, 6, 6);
}
static __inline__ __attribute__((always_inline)) __m256d _mm256_movedup_pd(__m256d a) {
  return (__m256d)__builtin_shufflevector(a, a, 0, 0, 2, 2);
}
static __inline__ __attribute__((always_inline)) __m256 _mm256_zextps128_ps256(__m128 a) {
  return (__m256){a[0], a[1], a[2], a[3], 0, 0, 0, 0};
}
static __inline__ __attribute__((always_inline)) __m256 _mm256_castps128_ps256(__m128 a) {
  return _mm256_zextps128_ps256(a);
}
static __inline__ __attribute__((always_inline)) __m256 _mm256_set_m128(__m128 hi, __m128 lo) {
  return (__m256){lo[0], lo[1], lo[2], lo[3], hi[0], hi[1], hi[2], hi[3]};
}
static __inline__ __attribute__((always_inline)) __m256 _mm256_setr_m128(__m128 lo, __m128 hi) {
  return (__m256){lo[0], lo[1], lo[2], lo[3], hi[0], hi[1], hi[2], hi[3]};
}
static __inline__ __attribute__((always_inline)) __m256d _mm256_zextpd128_pd256(__m128d a) {
  return (__m256d){a[0], a[1], 0, 0};
}
static __inline__ __attribute__((always_inline)) __m256d _mm256_castpd128_pd256(__m128d a) {
  return _mm256_zextpd128_pd256(a);
}
static __inline__ __attribute__((always_inline)) __m256d _mm256_set_m128d(__m128d hi, __m128d lo) {
  return (__m256d){lo[0], lo[1], hi[0], hi[1]};
}
static __inline__ __attribute__((always_inline)) __m256d _mm256_setr_m128d(__m128d lo, __m128d hi) {
  return (__m256d){lo[0], lo[1], hi[0], hi[1]};
}
static __inline__ __attribute__((always_inline)) __m256i _mm256_zextsi128_si256(__m128i a) {
  return (__m256i){a[0], a[1], 0, 0};
}
static __inline__ __attribute__((always_inline)) __m256i _mm256_castsi128_si256(__m128i a) {
  return _mm256_zextsi128_si256(a);
}
static __inline__ __attribute__((always_inline)) __m256i _mm256_set_m128i(__m128i hi, __m128i lo) {
  return (__m256i){lo[0], lo[1], hi[0], hi[1]};
}
static __inline__ __attribute__((always_inline)) __m256i _mm256_setr_m128i(__m128i lo, __m128i hi) {
  return (__m256i){lo[0], lo[1], hi[0], hi[1]};
}
static __inline__ __attribute__((always_inline)) __m128 _mm_broadcast_ss(const float *p) {
  float a; __builtin_memcpy(&a,p,4); return (__m128){a, a, a, a};
}
static __inline__ __attribute__((always_inline)) __m256 _mm256_broadcast_ss(const float *p) {
  float a; __builtin_memcpy(&a,p,4); return (__m256){a, a, a, a, a, a, a, a};
}
static __inline__ __attribute__((always_inline)) __m256d _mm256_broadcast_sd(const double *p) {
  double a; __builtin_memcpy(&a,p,8); return (__m256d){a, a, a, a};
}
static __inline__ __attribute__((always_inline)) __m256 _mm256_broadcast_ps(const __m128 *p) {
  __m128 a; __builtin_memcpy(&a,p,16); return (__m256){a[0], a[1], a[2], a[3], a[0], a[1], a[2], a[3]};
}
static __inline__ __attribute__((always_inline)) __m256d _mm256_broadcast_pd(const __m128d *p) {
  __m128d a; __builtin_memcpy(&a,p,16); return (__m256d){a[0], a[1], a[0], a[1]};
}
#endif /* _KEEL_AVX_CONSTRUCT_H */
