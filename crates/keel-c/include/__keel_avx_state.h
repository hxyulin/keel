/* Architectural state and non-temporal transfers. */
#ifndef _KEEL_AVX_STATE_H
#define _KEEL_AVX_STATE_H
static __inline__ __attribute__((always_inline)) void _mm256_zeroall(void) { __builtin_keel_avx_vzeroall(); }
static __inline__ __attribute__((always_inline)) void _mm256_stream_ps(float*p,__m256 a) { __builtin_keel_avx_vmovntps256((void*)p,a); }
static __inline__ __attribute__((always_inline)) void _mm256_stream_pd(double*p,__m256d a) { __builtin_keel_avx_vmovntpd256((void*)p,a); }
static __inline__ __attribute__((always_inline)) void _mm256_stream_si256(void*p,__m256i a) { __builtin_keel_avx_vmovntdq256((void*)p,a); }
static __inline__ __attribute__((always_inline)) void _mm256_zeroupper(void) { __asm__ __volatile__("vzeroupper" ::: "ymm0","ymm1","ymm2","ymm3","ymm4","ymm5","ymm6","ymm7","ymm8","ymm9","ymm10","ymm11","ymm12","ymm13","ymm14","ymm15"); }
#endif
