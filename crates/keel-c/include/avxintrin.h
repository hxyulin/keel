/* Bounded Keel intrinsic subset; see docs/c-x86-intrinsics.md. */
#ifndef _KEEL_AVXINTRIN_H
#define _KEEL_AVXINTRIN_H
#ifndef __x86_64__
#error "Keel x86 intrinsic headers require x86-64"
#endif
#include <smmintrin.h>
typedef float __m256 __attribute__((vector_size(32)));
typedef float __m256_u __attribute__((vector_size(32), aligned(1)));
typedef double __m256d __attribute__((vector_size(32)));
typedef double __m256d_u __attribute__((vector_size(32), aligned(1)));
typedef long long __m256i __attribute__((vector_size(32)));
typedef long long __m256i_u __attribute__((vector_size(32), aligned(1)));
typedef signed char __keel_i8_256 __attribute__((vector_size(32)));
typedef unsigned char __keel_u8_256 __attribute__((vector_size(32)));
typedef signed short __keel_i16_256 __attribute__((vector_size(32)));
typedef unsigned short __keel_u16_256 __attribute__((vector_size(32)));
typedef signed int __keel_i32_256 __attribute__((vector_size(32)));
typedef unsigned int __keel_u32_256 __attribute__((vector_size(32)));
typedef signed long long __keel_i64_256 __attribute__((vector_size(32)));
typedef unsigned long long __keel_u64_256 __attribute__((vector_size(32)));
static __inline__ __attribute__((always_inline)) __m256 _mm256_add_ps(__m256 a, __m256 b) { return a + b; }
static __inline__ __attribute__((always_inline)) __m256 _mm256_sub_ps(__m256 a, __m256 b) { return a - b; }
static __inline__ __attribute__((always_inline)) __m256 _mm256_mul_ps(__m256 a, __m256 b) { return a * b; }
static __inline__ __attribute__((always_inline)) __m256 _mm256_div_ps(__m256 a, __m256 b) { return a / b; }
static __inline__ __attribute__((always_inline)) __m256 _mm256_setzero_ps(void) { return (__m256){0}; }
static __inline__ __attribute__((always_inline)) __m256 _mm256_set1_ps(float x) {
  return (__m256){x, x, x, x, x, x, x, x};
}
static __inline__ __attribute__((always_inline)) __m256 _mm256_set_ps(float x7, float x6, float x5, float x4,
                                       float x3, float x2, float x1, float x0) {
  return (__m256){x0, x1, x2, x3, x4, x5, x6, x7};
}
static __inline__ __attribute__((always_inline)) __m256 _mm256_load_ps(const float *p) {
  __m256 r;
  __builtin_memcpy(&r, p, 32);
  return r;
}
static __inline__ __attribute__((always_inline)) void _mm256_store_ps(float *p, __m256 a) {
  __builtin_memcpy(p, &a, 32);
}
static __inline__ __attribute__((always_inline)) __m256 _mm256_loadu_ps(const float *p) {
  __m256 r;
  __builtin_memcpy(&r, p, 32);
  return r;
}
static __inline__ __attribute__((always_inline)) void _mm256_storeu_ps(float *p, __m256 a) {
  __builtin_memcpy(p, &a, 32);
}
static __inline__ __attribute__((always_inline)) __m256d _mm256_add_pd(__m256d a, __m256d b) { return a + b; }
static __inline__ __attribute__((always_inline)) __m256d _mm256_sub_pd(__m256d a, __m256d b) { return a - b; }
static __inline__ __attribute__((always_inline)) __m256d _mm256_mul_pd(__m256d a, __m256d b) { return a * b; }
static __inline__ __attribute__((always_inline)) __m256d _mm256_div_pd(__m256d a, __m256d b) { return a / b; }
static __inline__ __attribute__((always_inline)) __m256d _mm256_setzero_pd(void) { return (__m256d){0}; }
static __inline__ __attribute__((always_inline)) __m256d _mm256_set1_pd(double x) {
  return (__m256d){x, x, x, x};
}
static __inline__ __attribute__((always_inline)) __m256d _mm256_set_pd(double x3, double x2, double x1,
                                        double x0) {
  return (__m256d){x0, x1, x2, x3};
}
static __inline__ __attribute__((always_inline)) __m256d _mm256_load_pd(const double *p) {
  __m256d r;
  __builtin_memcpy(&r, p, 32);
  return r;
}
static __inline__ __attribute__((always_inline)) void _mm256_store_pd(double *p, __m256d a) {
  __builtin_memcpy(p, &a, 32);
}
static __inline__ __attribute__((always_inline)) __m256d _mm256_loadu_pd(const double *p) {
  __m256d r;
  __builtin_memcpy(&r, p, 32);
  return r;
}
static __inline__ __attribute__((always_inline)) void _mm256_storeu_pd(double *p, __m256d a) {
  __builtin_memcpy(p, &a, 32);
}
static __inline__ __attribute__((always_inline)) __m256d _mm256_castps_pd(__m256 a) { return (__m256d)a; }
static __inline__ __attribute__((always_inline)) __m256i _mm256_castps_si256(__m256 a) { return (__m256i)a; }
static __inline__ __attribute__((always_inline)) __m256 _mm256_castpd_ps(__m256d a) { return (__m256)a; }
static __inline__ __attribute__((always_inline)) __m256i _mm256_castpd_si256(__m256d a) { return (__m256i)a; }
static __inline__ __attribute__((always_inline)) __m256 _mm256_castsi256_ps(__m256i a) { return (__m256)a; }
static __inline__ __attribute__((always_inline)) __m256d _mm256_castsi256_pd(__m256i a) { return (__m256d)a; }
static __inline__ __attribute__((always_inline)) __m128 _mm256_extractf128_ps(__m256 a, const int i) {
  return (__m128){a[(i & 1) * 4 + 0], a[(i & 1) * 4 + 1], a[(i & 1) * 4 + 2],
                  a[(i & 1) * 4 + 3]};
}
static __inline__ __attribute__((always_inline)) __m256 _mm256_insertf128_ps(__m256 a, __m128 b, const int i) {
  for (int j = 0; j < 4; j++)
    a[(i & 1) * 4 + j] = b[j];
  return a;
}
static __inline__ __attribute__((always_inline)) __m128 _mm256_castps256_ps128(__m256 a) {
  return (__m128){a[0], a[1], a[2], a[3]};
}
static __inline__ __attribute__((always_inline)) __m128d _mm256_extractf128_pd(__m256d a, const int i) {
  return (__m128d){a[(i & 1) * 2 + 0], a[(i & 1) * 2 + 1]};
}
static __inline__ __attribute__((always_inline)) __m256d _mm256_insertf128_pd(__m256d a, __m128d b,
                                               const int i) {
  for (int j = 0; j < 2; j++)
    a[(i & 1) * 2 + j] = b[j];
  return a;
}
static __inline__ __attribute__((always_inline)) __m128d _mm256_castpd256_pd128(__m256d a) {
  return (__m128d){a[0], a[1]};
}
static __inline__ __attribute__((always_inline)) __m128i _mm256_extractf128_si256(__m256i a, const int i) {
  return (__m128i){a[(i & 1) * 2 + 0], a[(i & 1) * 2 + 1]};
}
static __inline__ __attribute__((always_inline)) __m256i _mm256_insertf128_si256(__m256i a, __m128i b,
                                                  const int i) {
  for (int j = 0; j < 2; j++)
    a[(i & 1) * 2 + j] = b[j];
  return a;
}
static __inline__ __attribute__((always_inline)) __m128i _mm256_castsi256_si128(__m256i a) {
  return (__m128i){a[0], a[1]};
}
#define _mm256_shuffle_ps(a, b, i)                                             \
  __builtin_shufflevector((a), (b), (i) & 3, ((i) >> 2) & 3,                   \
                          8 + (((i) >> 4) & 3), 8 + (((i) >> 6) & 3),          \
                          4 + ((i) & 3), 4 + (((i) >> 2) & 3),                 \
                          12 + (((i) >> 4) & 3), 12 + (((i) >> 6) & 3))


static __inline__ __attribute__((always_inline)) __m256 _mm256_and_ps(__m256 a, __m256 b) {
  return (__m256)((__m256i)a & (__m256i)b);
}
static __inline__ __attribute__((always_inline)) __m256 _mm256_or_ps(__m256 a, __m256 b) {
  return (__m256)((__m256i)a | (__m256i)b);
}
static __inline__ __attribute__((always_inline)) __m256 _mm256_xor_ps(__m256 a, __m256 b) {
  return (__m256)((__m256i)a ^ (__m256i)b);
}
static __inline__ __attribute__((always_inline)) __m256 _mm256_andnot_ps(__m256 a, __m256 b) {
  return (__m256)(~(__m256i)a & (__m256i)b);
}
static __inline__ __attribute__((always_inline)) __m256d _mm256_and_pd(__m256d a, __m256d b) {
  return (__m256d)((__m256i)a & (__m256i)b);
}
static __inline__ __attribute__((always_inline)) __m256d _mm256_or_pd(__m256d a, __m256d b) {
  return (__m256d)((__m256i)a | (__m256i)b);
}
static __inline__ __attribute__((always_inline)) __m256d _mm256_xor_pd(__m256d a, __m256d b) {
  return (__m256d)((__m256i)a ^ (__m256i)b);
}
static __inline__ __attribute__((always_inline)) __m256d _mm256_andnot_pd(__m256d a, __m256d b) {
  return (__m256d)(~(__m256i)a & (__m256i)b);
}

static __inline__ __attribute__((always_inline)) __m256 _mm256_hadd_ps(__m256 a, __m256 b) {
  return __builtin_keel_avx_vhaddps256(a, b);
}
static __inline__ __attribute__((always_inline)) __m256 _mm256_hsub_ps(__m256 a, __m256 b) {
  return __builtin_keel_avx_vhsubps256(a, b);
}
static __inline__ __attribute__((always_inline)) __m256d _mm256_hadd_pd(__m256d a, __m256d b) {
  return __builtin_keel_avx_vhaddpd256(a, b);
}
static __inline__ __attribute__((always_inline)) __m256d _mm256_hsub_pd(__m256d a, __m256d b) {
  return __builtin_keel_avx_vhsubpd256(a, b);
}
static __inline__ __attribute__((always_inline)) __m256 _mm256_unpacklo_ps(__m256 a, __m256 b) {
  return (__m256)__builtin_shufflevector(a, b, 0, 8, 1, 9, 4, 12, 5, 13);
}
static __inline__ __attribute__((always_inline)) __m256 _mm256_unpackhi_ps(__m256 a, __m256 b) {
  return (__m256)__builtin_shufflevector(a, b, 2, 10, 3, 11, 6, 14, 7, 15);
}
static __inline__ __attribute__((always_inline)) __m256d _mm256_unpacklo_pd(__m256d a, __m256d b) {
  return (__m256d)__builtin_shufflevector(a, b, 0, 4, 2, 6);
}
static __inline__ __attribute__((always_inline)) __m256d _mm256_unpackhi_pd(__m256d a, __m256d b) {
  return (__m256d)__builtin_shufflevector(a, b, 1, 5, 3, 7);
}

static __inline__ __attribute__((always_inline)) __m256 _mm256_sqrt_ps(__m256 a) {
  return __builtin_keel_avx_vsqrtps256(a);
}
static __inline__ __attribute__((always_inline)) __m256 _mm256_addsub_ps(__m256 a, __m256 b) {
  return __builtin_keel_avx_vaddsubps256(a, b);
}
static __inline__ __attribute__((always_inline)) __m256d _mm256_sqrt_pd(__m256d a) {
  return __builtin_keel_avx_vsqrtpd256(a);
}
static __inline__ __attribute__((always_inline)) __m256d _mm256_addsub_pd(__m256d a, __m256d b) {
  return __builtin_keel_avx_vaddsubpd256(a, b);
}

#define _mm256_round_ps(a, mode) __builtin_keel_avx_vroundps256((__m256)(a), (mode))
static __inline__ __attribute__((always_inline)) __m256 _mm256_ceil_ps(__m256 a) {
  return _mm256_round_ps(a, _MM_FROUND_CEIL);
}
static __inline__ __attribute__((always_inline)) __m256 _mm256_floor_ps(__m256 a) {
  return _mm256_round_ps(a, _MM_FROUND_FLOOR);
}
#define _mm256_round_pd(a, mode) __builtin_keel_avx_vroundpd256((__m256d)(a), (mode))
static __inline__ __attribute__((always_inline)) __m256d _mm256_ceil_pd(__m256d a) {
  return _mm256_round_pd(a, _MM_FROUND_CEIL);
}
static __inline__ __attribute__((always_inline)) __m256d _mm256_floor_pd(__m256d a) {
  return _mm256_round_pd(a, _MM_FROUND_FLOOR);
}
static __inline__ __attribute__((always_inline)) __m256 _mm256_blendv_ps(__m256 a, __m256 b, __m256 mask) {
  return __builtin_keel_avx_vblendvps256(a, b, mask);
}
static __inline__ __attribute__((always_inline)) __m256d _mm256_blendv_pd(__m256d a, __m256d b, __m256d mask) {
  return __builtin_keel_avx_vblendvpd256(a, b, mask);
}

#include <__keel_avx_lane_ops.h>
#include <__keel_avx_construct.h>

#include <__keel_avx_float.h>

#include <__keel_avx_conversions.h>

#include <__keel_avx_queries.h>

#include <__keel_avx_compare.h>

#include <__keel_avx_mask_memory.h>

#include <__keel_avx_state.h>

#include <__keel_avx_convenience.h>

#endif /* _KEEL_AVXINTRIN_H */
