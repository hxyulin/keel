/* Architectural floating operations; keep operand order and MXCSR effects. */
#ifndef _KEEL_KEEL_AVX_FLOAT_H
#define _KEEL_KEEL_AVX_FLOAT_H
static __inline__ __attribute__((always_inline)) __m256 _mm256_min_ps(__m256 a, __m256 b) { return __builtin_keel_avx_vminps256(a,b); }
static __inline__ __attribute__((always_inline)) __m256 _mm256_max_ps(__m256 a, __m256 b) { return __builtin_keel_avx_vmaxps256(a,b); }
static __inline__ __attribute__((always_inline)) int _mm256_movemask_ps(__m256 a) { return __builtin_keel_avx_vmovmskps256(a); }
static __inline__ __attribute__((always_inline)) __m256d _mm256_min_pd(__m256d a, __m256d b) { return __builtin_keel_avx_vminpd256(a,b); }
static __inline__ __attribute__((always_inline)) __m256d _mm256_max_pd(__m256d a, __m256d b) { return __builtin_keel_avx_vmaxpd256(a,b); }
static __inline__ __attribute__((always_inline)) int _mm256_movemask_pd(__m256d a) { return __builtin_keel_avx_vmovmskpd256(a); }
static __inline__ __attribute__((always_inline)) __m256 _mm256_rcp_ps(__m256 a) { return __builtin_keel_avx_vrcpps256(a); }
static __inline__ __attribute__((always_inline)) __m256 _mm256_rsqrt_ps(__m256 a) { return __builtin_keel_avx_vrsqrtps256(a); }
#endif
