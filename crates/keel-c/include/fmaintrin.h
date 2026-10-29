/* Keel FMA wrappers. See docs/targets/x86_64/intrinsics.md. */
#ifndef _KEEL_FMAINTRIN_H
#define _KEEL_FMAINTRIN_H
#include <avxintrin.h>

static __inline__ __attribute__((always_inline)) __m128 _mm_fmadd_ps(__m128 a, __m128 b, __m128 c) { return __builtin_keel_x86_fmadd_ps128(a, b, c); }
static __inline__ __attribute__((always_inline)) __m128d _mm_fmadd_pd(__m128d a, __m128d b, __m128d c) { return __builtin_keel_x86_fmadd_pd128(a, b, c); }
static __inline__ __attribute__((always_inline)) __m256 _mm256_fmadd_ps(__m256 a, __m256 b, __m256 c) { return __builtin_keel_x86_fmadd_ps256(a, b, c); }
static __inline__ __attribute__((always_inline)) __m256d _mm256_fmadd_pd(__m256d a, __m256d b, __m256d c) { return __builtin_keel_x86_fmadd_pd256(a, b, c); }
static __inline__ __attribute__((always_inline)) __m128 _mm_fmadd_ss(__m128 a, __m128 b, __m128 c) { a[0] = __builtin_keel_x86_fmadd_ss(a[0], b[0], c[0]); return a; }
static __inline__ __attribute__((always_inline)) __m128d _mm_fmadd_sd(__m128d a, __m128d b, __m128d c) { a[0] = __builtin_keel_x86_fmadd_sd(a[0], b[0], c[0]); return a; }
static __inline__ __attribute__((always_inline)) __m128 _mm_fmsub_ps(__m128 a, __m128 b, __m128 c) { return __builtin_keel_x86_fmsub_ps128(a, b, c); }
static __inline__ __attribute__((always_inline)) __m128d _mm_fmsub_pd(__m128d a, __m128d b, __m128d c) { return __builtin_keel_x86_fmsub_pd128(a, b, c); }
static __inline__ __attribute__((always_inline)) __m256 _mm256_fmsub_ps(__m256 a, __m256 b, __m256 c) { return __builtin_keel_x86_fmsub_ps256(a, b, c); }
static __inline__ __attribute__((always_inline)) __m256d _mm256_fmsub_pd(__m256d a, __m256d b, __m256d c) { return __builtin_keel_x86_fmsub_pd256(a, b, c); }
static __inline__ __attribute__((always_inline)) __m128 _mm_fmsub_ss(__m128 a, __m128 b, __m128 c) { a[0] = __builtin_keel_x86_fmsub_ss(a[0], b[0], c[0]); return a; }
static __inline__ __attribute__((always_inline)) __m128d _mm_fmsub_sd(__m128d a, __m128d b, __m128d c) { a[0] = __builtin_keel_x86_fmsub_sd(a[0], b[0], c[0]); return a; }
static __inline__ __attribute__((always_inline)) __m128 _mm_fnmadd_ps(__m128 a, __m128 b, __m128 c) { return __builtin_keel_x86_fnmadd_ps128(a, b, c); }
static __inline__ __attribute__((always_inline)) __m128d _mm_fnmadd_pd(__m128d a, __m128d b, __m128d c) { return __builtin_keel_x86_fnmadd_pd128(a, b, c); }
static __inline__ __attribute__((always_inline)) __m256 _mm256_fnmadd_ps(__m256 a, __m256 b, __m256 c) { return __builtin_keel_x86_fnmadd_ps256(a, b, c); }
static __inline__ __attribute__((always_inline)) __m256d _mm256_fnmadd_pd(__m256d a, __m256d b, __m256d c) { return __builtin_keel_x86_fnmadd_pd256(a, b, c); }
static __inline__ __attribute__((always_inline)) __m128 _mm_fnmadd_ss(__m128 a, __m128 b, __m128 c) { a[0] = __builtin_keel_x86_fnmadd_ss(a[0], b[0], c[0]); return a; }
static __inline__ __attribute__((always_inline)) __m128d _mm_fnmadd_sd(__m128d a, __m128d b, __m128d c) { a[0] = __builtin_keel_x86_fnmadd_sd(a[0], b[0], c[0]); return a; }
static __inline__ __attribute__((always_inline)) __m128 _mm_fnmsub_ps(__m128 a, __m128 b, __m128 c) { return __builtin_keel_x86_fnmsub_ps128(a, b, c); }
static __inline__ __attribute__((always_inline)) __m128d _mm_fnmsub_pd(__m128d a, __m128d b, __m128d c) { return __builtin_keel_x86_fnmsub_pd128(a, b, c); }
static __inline__ __attribute__((always_inline)) __m256 _mm256_fnmsub_ps(__m256 a, __m256 b, __m256 c) { return __builtin_keel_x86_fnmsub_ps256(a, b, c); }
static __inline__ __attribute__((always_inline)) __m256d _mm256_fnmsub_pd(__m256d a, __m256d b, __m256d c) { return __builtin_keel_x86_fnmsub_pd256(a, b, c); }
static __inline__ __attribute__((always_inline)) __m128 _mm_fnmsub_ss(__m128 a, __m128 b, __m128 c) { a[0] = __builtin_keel_x86_fnmsub_ss(a[0], b[0], c[0]); return a; }
static __inline__ __attribute__((always_inline)) __m128d _mm_fnmsub_sd(__m128d a, __m128d b, __m128d c) { a[0] = __builtin_keel_x86_fnmsub_sd(a[0], b[0], c[0]); return a; }
static __inline__ __attribute__((always_inline)) __m128 _mm_fmaddsub_ps(__m128 a, __m128 b, __m128 c) { return __builtin_keel_x86_fmaddsub_ps128(a, b, c); }
static __inline__ __attribute__((always_inline)) __m128d _mm_fmaddsub_pd(__m128d a, __m128d b, __m128d c) { return __builtin_keel_x86_fmaddsub_pd128(a, b, c); }
static __inline__ __attribute__((always_inline)) __m256 _mm256_fmaddsub_ps(__m256 a, __m256 b, __m256 c) { return __builtin_keel_x86_fmaddsub_ps256(a, b, c); }
static __inline__ __attribute__((always_inline)) __m256d _mm256_fmaddsub_pd(__m256d a, __m256d b, __m256d c) { return __builtin_keel_x86_fmaddsub_pd256(a, b, c); }
static __inline__ __attribute__((always_inline)) __m128 _mm_fmsubadd_ps(__m128 a, __m128 b, __m128 c) { return __builtin_keel_x86_fmsubadd_ps128(a, b, c); }
static __inline__ __attribute__((always_inline)) __m128d _mm_fmsubadd_pd(__m128d a, __m128d b, __m128d c) { return __builtin_keel_x86_fmsubadd_pd128(a, b, c); }
static __inline__ __attribute__((always_inline)) __m256 _mm256_fmsubadd_ps(__m256 a, __m256 b, __m256 c) { return __builtin_keel_x86_fmsubadd_ps256(a, b, c); }
static __inline__ __attribute__((always_inline)) __m256d _mm256_fmsubadd_pd(__m256d a, __m256d b, __m256d c) { return __builtin_keel_x86_fmsubadd_pd256(a, b, c); }

#endif /* _KEEL_FMAINTRIN_H */
