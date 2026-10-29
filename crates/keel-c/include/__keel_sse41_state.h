/* Architectural state and non-temporal transfers. */
#ifndef _KEEL_SSE41_STATE_H
#define _KEEL_SSE41_STATE_H
static __inline__ __attribute__((always_inline)) __m128i _mm_stream_load_si128(const void*p) { return (__m128i)__builtin_keel_sse41_movntdqa128((void*)p); }
#endif
