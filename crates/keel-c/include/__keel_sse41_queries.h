#ifndef _KEEL_SSE41_QUERIES_H
#define _KEEL_SSE41_QUERIES_H
static __inline__ __attribute__((always_inline)) int _mm_testz_si128(__m128i a, __m128i b) { int flags=__builtin_keel_query_sse41ptest128((__keel_i8_128)a,(__keel_i8_128)b); return (flags >> 6) & 1; }
static __inline__ __attribute__((always_inline)) int _mm_testc_si128(__m128i a, __m128i b) { int flags=__builtin_keel_query_sse41ptest128((__keel_i8_128)a,(__keel_i8_128)b); return flags & 1; }
static __inline__ __attribute__((always_inline)) int _mm_testnzc_si128(__m128i a, __m128i b) { int flags=__builtin_keel_query_sse41ptest128((__keel_i8_128)a,(__keel_i8_128)b); return (flags & 65) == 0; }
#define _mm_test_all_zeros(a,b) _mm_testz_si128((a),(b))
#define _mm_test_mix_ones_zeros(a,b) _mm_testnzc_si128((a),(b))
static __inline__ __attribute__((always_inline)) int _mm_test_all_ones(__m128i a) { return _mm_testc_si128(a,_mm_set1_epi32(-1)); }
static __inline__ __attribute__((always_inline)) __m128i _mm_minpos_epu16(__m128i a) { return (__m128i)__builtin_keel_query_sse41phminposuw128((__keel_i16_128)a); }
#define _mm_mpsadbw_epu8(a,b,i) ((__m128i)__builtin_keel_query_sse41mpsadbw128((__keel_i8_128)(a),(__keel_i8_128)(b),(i)))
#define _mm_dp_ps(a,b,i) __builtin_keel_query_sse41dpps((__m128)(a),(__m128)(b),(i))
#define _mm_dp_pd(a,b,i) __builtin_keel_query_sse41dppd((__m128d)(a),(__m128d)(b),(i))
#endif /* _KEEL_SSE41_QUERIES_H */
