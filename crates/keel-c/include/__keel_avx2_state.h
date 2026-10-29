/* Architectural state and non-temporal transfers. */
#ifndef _KEEL_AVX2_STATE_H
#define _KEEL_AVX2_STATE_H
static __inline__ __attribute__((always_inline)) __m256i _mm256_stream_load_si256(const void*p) { return (__m256i)__builtin_keel_avx2_vmovntdqa256((void*)p); }
#endif
