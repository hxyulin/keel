/* Architectural state and non-temporal transfers. */
#ifndef _KEEL_SSE2_STATE_H
#define _KEEL_SSE2_STATE_H
static __inline__ __attribute__((always_inline)) void _mm_lfence(void) { __builtin_keel_lfence(); }
static __inline__ __attribute__((always_inline)) void _mm_mfence(void) { __builtin_keel_mfence(); }
static __inline__ __attribute__((always_inline)) void _mm_pause(void) { __builtin_keel_pause(); }
static __inline__ __attribute__((always_inline)) void _mm_stream_pd(double*p,__m128d a) { __builtin_keel_sse_movntpd128((void*)p,a); }
static __inline__ __attribute__((always_inline)) void _mm_stream_si128(void*p,__m128i a) { __builtin_keel_sse_movntdq128((void*)p,a); }
static __inline__ __attribute__((always_inline)) void _mm_stream_si32(void*p,int value) { __builtin_keel_movnti32(p,value); }
static __inline__ __attribute__((always_inline)) void _mm_stream_si64(void*p,long long value) { __builtin_keel_movnti64(p,value); }
static __inline__ __attribute__((always_inline)) void _mm_maskmoveu_si128(__m128i data,__m128i mask,char*address) {
  __builtin_keel_sse_maskmovdqu((void*)address,(__keel_i8_128)data,(__keel_i8_128)mask);
}
#endif
