/* Bounded Keel intrinsic subset; see docs/c-x86-intrinsics.md. */
#ifndef _KEEL_AVXINTRIN_H
#define _KEEL_AVXINTRIN_H
#ifndef __x86_64__
#error "Keel x86 intrinsic headers require x86-64"
#endif
#include <smmintrin.h>
typedef float __m256 __attribute__((vector_size(32)));
static __inline__ __m256 _mm256_add_ps(__m256 a, __m256 b) { return a + b; }
static __inline__ __m256 _mm256_sub_ps(__m256 a, __m256 b) { return a - b; }
static __inline__ __m256 _mm256_mul_ps(__m256 a, __m256 b) { return a * b; }
static __inline__ __m256 _mm256_div_ps(__m256 a, __m256 b) { return a / b; }
static __inline__ __m256 _mm256_setzero_ps(void) { return (__m256){0}; }
static __inline__ __m256 _mm256_set1_ps(float x) {
  return (__m256){x, x, x, x, x, x, x, x};
}
static __inline__ __m256 _mm256_set_ps(float x7, float x6, float x5, float x4,
                                       float x3, float x2, float x1, float x0) {
  return (__m256){x0, x1, x2, x3, x4, x5, x6, x7};
}
static __inline__ __m256 _mm256_load_ps(const float *p) {
  __m256 r;
  __builtin_memcpy(&r, p, 32);
  return r;
}
static __inline__ void _mm256_store_ps(float *p, __m256 a) {
  __builtin_memcpy(p, &a, 32);
}
static __inline__ __m256 _mm256_loadu_ps(const float *p) {
  __m256 r;
  __builtin_memcpy(&r, p, 32);
  return r;
}
static __inline__ void _mm256_storeu_ps(float *p, __m256 a) {
  __builtin_memcpy(p, &a, 32);
}
typedef double __m256d __attribute__((vector_size(32)));
static __inline__ __m256d _mm256_add_pd(__m256d a, __m256d b) { return a + b; }
static __inline__ __m256d _mm256_sub_pd(__m256d a, __m256d b) { return a - b; }
static __inline__ __m256d _mm256_mul_pd(__m256d a, __m256d b) { return a * b; }
static __inline__ __m256d _mm256_div_pd(__m256d a, __m256d b) { return a / b; }
static __inline__ __m256d _mm256_setzero_pd(void) { return (__m256d){0}; }
static __inline__ __m256d _mm256_set1_pd(double x) {
  return (__m256d){x, x, x, x};
}
static __inline__ __m256d _mm256_set_pd(double x3, double x2, double x1,
                                        double x0) {
  return (__m256d){x0, x1, x2, x3};
}
static __inline__ __m256d _mm256_load_pd(const double *p) {
  __m256d r;
  __builtin_memcpy(&r, p, 32);
  return r;
}
static __inline__ void _mm256_store_pd(double *p, __m256d a) {
  __builtin_memcpy(p, &a, 32);
}
static __inline__ __m256d _mm256_loadu_pd(const double *p) {
  __m256d r;
  __builtin_memcpy(&r, p, 32);
  return r;
}
static __inline__ void _mm256_storeu_pd(double *p, __m256d a) {
  __builtin_memcpy(p, &a, 32);
}
typedef long long __m256i __attribute__((vector_size(32)));
static __inline__ __m256d _mm256_castps_pd(__m256 a) { return (__m256d)a; }
static __inline__ __m256i _mm256_castps_si256(__m256 a) { return (__m256i)a; }
static __inline__ __m256 _mm256_castpd_ps(__m256d a) { return (__m256)a; }
static __inline__ __m256i _mm256_castpd_si256(__m256d a) { return (__m256i)a; }
static __inline__ __m256 _mm256_castsi256_ps(__m256i a) { return (__m256)a; }
static __inline__ __m256d _mm256_castsi256_pd(__m256i a) { return (__m256d)a; }
static __inline__ __m128 _mm256_extractf128_ps(__m256 a, const int i) {
  return (__m128){a[(i & 1) * 4 + 0], a[(i & 1) * 4 + 1], a[(i & 1) * 4 + 2],
                  a[(i & 1) * 4 + 3]};
}
static __inline__ __m256 _mm256_insertf128_ps(__m256 a, __m128 b, const int i) {
  for (int j = 0; j < 4; j++)
    a[(i & 1) * 4 + j] = b[j];
  return a;
}
static __inline__ __m128 _mm256_castps256_ps128(__m256 a) {
  return (__m128){a[0], a[1], a[2], a[3]};
}
static __inline__ __m128d _mm256_extractf128_pd(__m256d a, const int i) {
  return (__m128d){a[(i & 1) * 2 + 0], a[(i & 1) * 2 + 1]};
}
static __inline__ __m256d _mm256_insertf128_pd(__m256d a, __m128d b,
                                               const int i) {
  for (int j = 0; j < 2; j++)
    a[(i & 1) * 2 + j] = b[j];
  return a;
}
static __inline__ __m128d _mm256_castpd256_pd128(__m256d a) {
  return (__m128d){a[0], a[1]};
}
static __inline__ __m128i _mm256_extractf128_si256(__m256i a, const int i) {
  return (__m128i){a[(i & 1) * 2 + 0], a[(i & 1) * 2 + 1]};
}
static __inline__ __m256i _mm256_insertf128_si256(__m256i a, __m128i b,
                                                  const int i) {
  for (int j = 0; j < 2; j++)
    a[(i & 1) * 2 + j] = b[j];
  return a;
}
static __inline__ __m128i _mm256_castsi256_si128(__m256i a) {
  return (__m128i){a[0], a[1]};
}
#define _mm256_shuffle_ps(a, b, i)                                             \
  __builtin_shufflevector((a), (b), (i) & 3, ((i) >> 2) & 3,                   \
                          8 + (((i) >> 4) & 3), 8 + (((i) >> 6) & 3),          \
                          4 + ((i) & 3), 4 + (((i) >> 2) & 3),                 \
                          12 + (((i) >> 4) & 3), 12 + (((i) >> 6) & 3))

#define _CMP_EQ_OQ 0
#define _CMP_UNORD_Q 3
#define _CMP_NEQ_UQ 4
#define _CMP_ORD_Q 7
#define _CMP_LT_OQ 17
#define _CMP_LE_OQ 18
#define _CMP_GE_OQ 29
#define _CMP_GT_OQ 30
#define _mm256_cmp_ps(a, b, i) __builtin_ia32_cmpps256((a), (b), (i))
#define _mm256_cmp_pd(a, b, i) __builtin_ia32_cmppd256((a), (b), (i))

static __inline__ __m256 _mm256_and_ps(__m256 a, __m256 b) {
  return (__m256)((__m256i)a & (__m256i)b);
}
static __inline__ __m256 _mm256_or_ps(__m256 a, __m256 b) {
  return (__m256)((__m256i)a | (__m256i)b);
}
static __inline__ __m256 _mm256_xor_ps(__m256 a, __m256 b) {
  return (__m256)((__m256i)a ^ (__m256i)b);
}
static __inline__ __m256 _mm256_andnot_ps(__m256 a, __m256 b) {
  return (__m256)(~(__m256i)a & (__m256i)b);
}
static __inline__ __m256d _mm256_and_pd(__m256d a, __m256d b) {
  return (__m256d)((__m256i)a & (__m256i)b);
}
static __inline__ __m256d _mm256_or_pd(__m256d a, __m256d b) {
  return (__m256d)((__m256i)a | (__m256i)b);
}
static __inline__ __m256d _mm256_xor_pd(__m256d a, __m256d b) {
  return (__m256d)((__m256i)a ^ (__m256i)b);
}
static __inline__ __m256d _mm256_andnot_pd(__m256d a, __m256d b) {
  return (__m256d)(~(__m256i)a & (__m256i)b);
}

static __inline__ __m256 _mm256_hadd_ps(__m256 a, __m256 b) {
  return __builtin_keel_avx_vhaddps256(a, b);
}
static __inline__ __m256 _mm256_hsub_ps(__m256 a, __m256 b) {
  return __builtin_keel_avx_vhsubps256(a, b);
}
static __inline__ __m256d _mm256_hadd_pd(__m256d a, __m256d b) {
  return __builtin_keel_avx_vhaddpd256(a, b);
}
static __inline__ __m256d _mm256_hsub_pd(__m256d a, __m256d b) {
  return __builtin_keel_avx_vhsubpd256(a, b);
}
static __inline__ __m256 _mm256_unpacklo_ps(__m256 a, __m256 b) {
  return (__m256)__builtin_shufflevector(a, b, 0, 8, 1, 9, 4, 12, 5, 13);
}
static __inline__ __m256 _mm256_unpackhi_ps(__m256 a, __m256 b) {
  return (__m256)__builtin_shufflevector(a, b, 2, 10, 3, 11, 6, 14, 7, 15);
}
static __inline__ __m256d _mm256_unpacklo_pd(__m256d a, __m256d b) {
  return (__m256d)__builtin_shufflevector(a, b, 0, 4, 2, 6);
}
static __inline__ __m256d _mm256_unpackhi_pd(__m256d a, __m256d b) {
  return (__m256d)__builtin_shufflevector(a, b, 1, 5, 3, 7);
}

#endif /* _KEEL_AVXINTRIN_H */
