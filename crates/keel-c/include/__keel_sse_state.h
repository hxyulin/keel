/* Architectural state and non-temporal transfers. */
#ifndef _KEEL_SSE_STATE_H
#define _KEEL_SSE_STATE_H
static __inline__ __attribute__((always_inline)) unsigned _mm_getcsr(void) { return (unsigned)__builtin_keel_sse_read_mxcsr(); }
static __inline__ __attribute__((always_inline)) void _mm_setcsr(unsigned value) { (void)__builtin_keel_sse_swap_mxcsr((int)value); }
static __inline__ __attribute__((always_inline)) void _mm_sfence(void) { __builtin_keel_sfence(); }
static __inline__ __attribute__((always_inline)) void _mm_stream_ps(float*p,__m128 a) { __builtin_keel_sse_movntps128((void*)p,a); }
#define _MM_HINT_NTA 0
#define _MM_HINT_T2 1
#define _MM_HINT_T1 2
#define _MM_HINT_T0 3
#define _MM_HINT_ET1 6
#define _MM_HINT_ET0 7
#define _mm_prefetch(p,hint) __builtin_prefetch((const void*)(p),((hint)>>2)&1,(hint)&3)
#define _MM_GET_EXCEPTION_STATE() (_mm_getcsr() & 63u)
#define _MM_SET_EXCEPTION_STATE(value) _mm_setcsr((_mm_getcsr() & ~63u) | ((unsigned)(value) & 63u))
#define _MM_GET_EXCEPTION_MASK() (_mm_getcsr() & 8064u)
#define _MM_SET_EXCEPTION_MASK(value) _mm_setcsr((_mm_getcsr() & ~8064u) | ((unsigned)(value) & 8064u))
#define _MM_GET_ROUNDING_MODE() (_mm_getcsr() & 24576u)
#define _MM_SET_ROUNDING_MODE(value) _mm_setcsr((_mm_getcsr() & ~24576u) | ((unsigned)(value) & 24576u))
#define _MM_GET_FLUSH_ZERO_MODE() (_mm_getcsr() & 32768u)
#define _MM_SET_FLUSH_ZERO_MODE(value) _mm_setcsr((_mm_getcsr() & ~32768u) | ((unsigned)(value) & 32768u))
#define _MM_GET_DENORMALS_ZERO_MODE() (_mm_getcsr() & 64u)
#define _MM_SET_DENORMALS_ZERO_MODE(value) _mm_setcsr((_mm_getcsr() & ~64u) | ((unsigned)(value) & 64u))
#define _MM_EXCEPT_INVALID 1u
#define _MM_EXCEPT_DENORM 2u
#define _MM_EXCEPT_DIV_ZERO 4u
#define _MM_EXCEPT_OVERFLOW 8u
#define _MM_EXCEPT_UNDERFLOW 16u
#define _MM_EXCEPT_INEXACT 32u
#define _MM_MASK_INVALID 128u
#define _MM_MASK_DENORM 256u
#define _MM_MASK_DIV_ZERO 512u
#define _MM_MASK_OVERFLOW 1024u
#define _MM_MASK_UNDERFLOW 2048u
#define _MM_MASK_INEXACT 4096u
#define _MM_MASK_MASK 8064u
#define _MM_ROUND_NEAREST 0u
#define _MM_ROUND_DOWN 8192u
#define _MM_ROUND_UP 16384u
#define _MM_ROUND_TOWARD_ZERO 24576u
#define _MM_ROUND_MASK 24576u
#define _MM_FLUSH_ZERO_ON 32768u
#define _MM_FLUSH_ZERO_OFF 0u
#define _MM_FLUSH_ZERO_MASK 32768u
#define _MM_DENORMALS_ZERO_ON 64u
#define _MM_DENORMALS_ZERO_OFF 0u
#define _MM_DENORMALS_ZERO_MASK 64u
#define _MM_EXCEPT_MASK 63u
#endif
