/* Bounded Keel SSE3 intrinsic subset. */
#ifndef _KEEL_PMMINTRIN_H
#define _KEEL_PMMINTRIN_H
#ifndef __x86_64__
#error "Keel x86 intrinsic headers require x86-64"
#endif
#include <emmintrin.h>

static __inline__ __attribute__((always_inline)) __m128 _mm_hadd_ps(__m128 a, __m128 b) {
  return __builtin_keel_sse3_haddps(a, b);
}
static __inline__ __attribute__((always_inline)) __m128 _mm_hsub_ps(__m128 a, __m128 b) {
  return __builtin_keel_sse3_hsubps(a, b);
}
static __inline__ __attribute__((always_inline)) __m128d _mm_hadd_pd(__m128d a, __m128d b) {
  return __builtin_keel_sse3_haddpd(a, b);
}
static __inline__ __attribute__((always_inline)) __m128d _mm_hsub_pd(__m128d a, __m128d b) {
  return __builtin_keel_sse3_hsubpd(a, b);
}

static __inline__ __attribute__((always_inline)) __m128 _mm_addsub_ps(__m128 a, __m128 b) {
  return __builtin_keel_sse3_addsubps(a, b);
}
static __inline__ __attribute__((always_inline)) __m128d _mm_addsub_pd(__m128d a, __m128d b) {
  return __builtin_keel_sse3_addsubpd(a, b);
}

#include <__keel_sse3_duplicate.h>

#include <__keel_sse3_memory.h>

/* MONITOR/MWAIT require CPU support and execution-mode permission. */
static __inline__ __attribute__((always_inline)) void _mm_monitor(const void*address,unsigned extensions,unsigned hints) {
  __asm__ __volatile__("monitor" :: "a"(address),"c"(extensions),"d"(hints) : "memory");
}
static __inline__ __attribute__((always_inline)) void _mm_mwait(unsigned extensions,unsigned hints) {
  __asm__ __volatile__("mwait" :: "a"(hints),"c"(extensions) : "memory");
}

#include <__keel_sse3_convenience.h>

#endif /* _KEEL_PMMINTRIN_H */
