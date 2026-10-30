/* AVX512F scalar-mask wrappers. Each argument is evaluated once. */
#ifndef _KEEL_AVX512F_H
#define _KEEL_AVX512F_H
/* Included after the 512-bit vector aliases in immintrin.h. */
typedef unsigned char __mmask8;
typedef unsigned short __mmask16;
typedef int __keel_avx512_i32x16 __attribute__((vector_size(64)));
#define _mm512_cmp_epi32_mask(a,b,p) ((__mmask16)__builtin_keel_avx512_cmpd((__keel_avx512_i32x16)(a),(__keel_avx512_i32x16)(b),(p)))
#define _mm512_cmp_epu32_mask(a,b,p) ((__mmask16)__builtin_keel_avx512_cmpud((__keel_avx512_i32x16)(a),(__keel_avx512_i32x16)(b),(p)))
#define _mm512_mask_add_epi32(src,k,a,b) ((__m512i)__builtin_keel_avx512_maskaddd((__keel_avx512_i32x16)(src),(short)(k),(__keel_avx512_i32x16)(a),(__keel_avx512_i32x16)(b)))
#define _mm512_mask_sub_epi32(src,k,a,b) ((__m512i)__builtin_keel_avx512_masksubd((__keel_avx512_i32x16)(src),(short)(k),(__keel_avx512_i32x16)(a),(__keel_avx512_i32x16)(b)))
#define _mm512_mask_loadu_epi32(src,k,p) ((__m512i)__builtin_keel_avx512_maskloadd((void*)(p),(__keel_avx512_i32x16)(src),(short)(k)))
#define _mm512_mask_storeu_epi32(p,k,a) __builtin_keel_avx512_maskstored((void*)(p),(__keel_avx512_i32x16)(a),(short)(k))
#define _mm512_cmp_epi64_mask(a,b,p) ((__mmask8)__builtin_keel_avx512_cmpq((__m512i)(a),(__m512i)(b),(p)))
#define _mm512_cmp_epu64_mask(a,b,p) ((__mmask8)__builtin_keel_avx512_cmpuq((__m512i)(a),(__m512i)(b),(p)))
#define _mm512_mask_add_epi64(src,k,a,b) ((__m512i)__builtin_keel_avx512_maskaddq((__m512i)(src),(signed char)(k),(__m512i)(a),(__m512i)(b)))
#define _mm512_mask_sub_epi64(src,k,a,b) ((__m512i)__builtin_keel_avx512_masksubq((__m512i)(src),(signed char)(k),(__m512i)(a),(__m512i)(b)))
#define _mm512_mask_loadu_epi64(src,k,p) ((__m512i)__builtin_keel_avx512_maskloadq((void*)(p),(__m512i)(src),(signed char)(k)))
#define _mm512_mask_storeu_epi64(p,k,a) __builtin_keel_avx512_maskstoreq((void*)(p),(__m512i)(a),(signed char)(k))
#define _mm512_mask_cmp_ps_mask(k,a,b,p) ((__mmask16)__builtin_keel_avx512_cmpps((__m512)(a),(__m512)(b),(short)(k),(p)))
#define _mm512_cmp_ps_mask(a,b,p) _mm512_mask_cmp_ps_mask(0xffff,(a),(b),(p))
#define _mm512_mask_add_ps(src,k,a,b) __builtin_keel_avx512_maskaddps((__m512)(src),(short)(k),(__m512)(a),(__m512)(b))
#define _mm512_mask_sub_ps(src,k,a,b) __builtin_keel_avx512_masksubps((__m512)(src),(short)(k),(__m512)(a),(__m512)(b))
#define _mm512_mask_mul_ps(src,k,a,b) __builtin_keel_avx512_maskmulps((__m512)(src),(short)(k),(__m512)(a),(__m512)(b))
#define _mm512_mask_div_ps(src,k,a,b) __builtin_keel_avx512_maskdivps((__m512)(src),(short)(k),(__m512)(a),(__m512)(b))
#define _mm512_mask_sqrt_ps(src,k,a) __builtin_keel_avx512_masksqrtps((__m512)(src),(short)(k),(__m512)(a))
#define _mm512_mask_loadu_ps(src,k,p) __builtin_keel_avx512_maskloadps((void*)(p),(__m512)(src),(short)(k))
#define _mm512_mask_storeu_ps(p,k,a) __builtin_keel_avx512_maskstoreps((void*)(p),(__m512)(a),(short)(k))
#define _mm512_mask_cmp_pd_mask(k,a,b,p) ((__mmask8)__builtin_keel_avx512_cmppd((__m512d)(a),(__m512d)(b),(signed char)(k),(p)))
#define _mm512_cmp_pd_mask(a,b,p) _mm512_mask_cmp_pd_mask(0xff,(a),(b),(p))
#define _mm512_mask_add_pd(src,k,a,b) __builtin_keel_avx512_maskaddpd((__m512d)(src),(signed char)(k),(__m512d)(a),(__m512d)(b))
#define _mm512_mask_sub_pd(src,k,a,b) __builtin_keel_avx512_masksubpd((__m512d)(src),(signed char)(k),(__m512d)(a),(__m512d)(b))
#define _mm512_mask_mul_pd(src,k,a,b) __builtin_keel_avx512_maskmulpd((__m512d)(src),(signed char)(k),(__m512d)(a),(__m512d)(b))
#define _mm512_mask_div_pd(src,k,a,b) __builtin_keel_avx512_maskdivpd((__m512d)(src),(signed char)(k),(__m512d)(a),(__m512d)(b))
#define _mm512_mask_sqrt_pd(src,k,a) __builtin_keel_avx512_masksqrtpd((__m512d)(src),(signed char)(k),(__m512d)(a))
#define _mm512_mask_loadu_pd(src,k,p) __builtin_keel_avx512_maskloadpd((void*)(p),(__m512d)(src),(signed char)(k))
#define _mm512_mask_storeu_pd(p,k,a) __builtin_keel_avx512_maskstorepd((void*)(p),(__m512d)(a),(signed char)(k))

/* Unmasked operations and constructors share the typed 512-bit surface. */
static __inline__ __attribute__((always_inline)) __m512 _mm512_setzero_ps(void) { return (__m512){0}; }
static __inline__ __attribute__((always_inline)) __m512 _mm512_set1_ps(float x) { return (__m512){x, x, x, x, x, x, x, x, x, x, x, x, x, x, x, x}; }
static __inline__ __attribute__((always_inline)) __m512 _mm512_add_ps(__m512 a, __m512 b) { return _mm512_mask_add_ps((__m512){0}, 0xffff, a, b); }
static __inline__ __attribute__((always_inline)) __m512 _mm512_sub_ps(__m512 a, __m512 b) { return _mm512_mask_sub_ps((__m512){0}, 0xffff, a, b); }
static __inline__ __attribute__((always_inline)) __m512 _mm512_mul_ps(__m512 a, __m512 b) { return _mm512_mask_mul_ps((__m512){0}, 0xffff, a, b); }
static __inline__ __attribute__((always_inline)) __m512 _mm512_div_ps(__m512 a, __m512 b) { return _mm512_mask_div_ps((__m512){0}, 0xffff, a, b); }
static __inline__ __attribute__((always_inline)) __m512 _mm512_sqrt_ps(__m512 a) { return _mm512_mask_sqrt_ps((__m512){0}, 0xffff, a); }
static __inline__ __attribute__((always_inline)) __m512 _mm512_load_ps(const void *p) { return _mm512_mask_loadu_ps((__m512){0}, 0xffff, p); }
static __inline__ __attribute__((always_inline)) void _mm512_store_ps(void *p, __m512 a) { _mm512_mask_storeu_ps(p, 0xffff, a); }
static __inline__ __attribute__((always_inline)) __m512 _mm512_loadu_ps(const void *p) { return _mm512_mask_loadu_ps((__m512){0}, 0xffff, p); }
static __inline__ __attribute__((always_inline)) void _mm512_storeu_ps(void *p, __m512 a) { _mm512_mask_storeu_ps(p, 0xffff, a); }
static __inline__ __attribute__((always_inline)) __m512d _mm512_setzero_pd(void) { return (__m512d){0}; }
static __inline__ __attribute__((always_inline)) __m512d _mm512_set1_pd(double x) { return (__m512d){x, x, x, x, x, x, x, x}; }
static __inline__ __attribute__((always_inline)) __m512d _mm512_add_pd(__m512d a, __m512d b) { return _mm512_mask_add_pd((__m512d){0}, 0xff, a, b); }
static __inline__ __attribute__((always_inline)) __m512d _mm512_sub_pd(__m512d a, __m512d b) { return _mm512_mask_sub_pd((__m512d){0}, 0xff, a, b); }
static __inline__ __attribute__((always_inline)) __m512d _mm512_mul_pd(__m512d a, __m512d b) { return _mm512_mask_mul_pd((__m512d){0}, 0xff, a, b); }
static __inline__ __attribute__((always_inline)) __m512d _mm512_div_pd(__m512d a, __m512d b) { return _mm512_mask_div_pd((__m512d){0}, 0xff, a, b); }
static __inline__ __attribute__((always_inline)) __m512d _mm512_sqrt_pd(__m512d a) { return _mm512_mask_sqrt_pd((__m512d){0}, 0xff, a); }
static __inline__ __attribute__((always_inline)) __m512d _mm512_load_pd(const void *p) { return _mm512_mask_loadu_pd((__m512d){0}, 0xff, p); }
static __inline__ __attribute__((always_inline)) void _mm512_store_pd(void *p, __m512d a) { _mm512_mask_storeu_pd(p, 0xff, a); }
static __inline__ __attribute__((always_inline)) __m512d _mm512_loadu_pd(const void *p) { return _mm512_mask_loadu_pd((__m512d){0}, 0xff, p); }
static __inline__ __attribute__((always_inline)) void _mm512_storeu_pd(void *p, __m512d a) { _mm512_mask_storeu_pd(p, 0xff, a); }
static __inline__ __attribute__((always_inline)) __m512i _mm512_setzero_si512(void) { return (__m512i){0}; }
typedef unsigned int __keel_avx512_u32 __attribute__((vector_size(64)));
static __inline__ __attribute__((always_inline)) __m512i _mm512_set1_epi32(int x) { return (__m512i)(__keel_avx512_u32){x, x, x, x, x, x, x, x, x, x, x, x, x, x, x, x}; }
static __inline__ __attribute__((always_inline)) __m512i _mm512_add_epi32(__m512i a, __m512i b) { return (__m512i)((__keel_avx512_u32)a + (__keel_avx512_u32)b); }
static __inline__ __attribute__((always_inline)) __m512i _mm512_sub_epi32(__m512i a, __m512i b) { return (__m512i)((__keel_avx512_u32)a - (__keel_avx512_u32)b); }
static __inline__ __attribute__((always_inline)) __m512i _mm512_mullo_epi32(__m512i a, __m512i b) { return (__m512i)((__keel_avx512_u32)a * (__keel_avx512_u32)b); }
typedef unsigned long long __keel_avx512_u64 __attribute__((vector_size(64)));
static __inline__ __attribute__((always_inline)) __m512i _mm512_set1_epi64(long long x) { return (__m512i)(__keel_avx512_u64){x, x, x, x, x, x, x, x}; }
static __inline__ __attribute__((always_inline)) __m512i _mm512_add_epi64(__m512i a, __m512i b) { return (__m512i)((__keel_avx512_u64)a + (__keel_avx512_u64)b); }
static __inline__ __attribute__((always_inline)) __m512i _mm512_sub_epi64(__m512i a, __m512i b) { return (__m512i)((__keel_avx512_u64)a - (__keel_avx512_u64)b); }
static __inline__ __attribute__((always_inline)) __m512i _mm512_and_si512(__m512i a, __m512i b) { return a & b; }
static __inline__ __attribute__((always_inline)) __m512i _mm512_or_si512(__m512i a, __m512i b) { return a | b; }
static __inline__ __attribute__((always_inline)) __m512i _mm512_xor_si512(__m512i a, __m512i b) { return a ^ b; }
static __inline__ __attribute__((always_inline)) __m512i _mm512_andnot_si512(__m512i a, __m512i b) { return ~a & b; }
static __inline__ __attribute__((always_inline)) __m512i _mm512_load_si512(const void *p) { return _mm512_mask_loadu_epi64((__m512i){0}, 0xff, p); }
static __inline__ __attribute__((always_inline)) void _mm512_store_si512(void *p, __m512i a) { _mm512_mask_storeu_epi64(p, 0xff, a); }
static __inline__ __attribute__((always_inline)) __m512i _mm512_loadu_si512(const void *p) { return _mm512_mask_loadu_epi64((__m512i){0}, 0xff, p); }
static __inline__ __attribute__((always_inline)) void _mm512_storeu_si512(void *p, __m512i a) { _mm512_mask_storeu_epi64(p, 0xff, a); }
static __inline__ __attribute__((always_inline)) __m512d _mm512_castps_pd(__m512 a) { return (__m512d)a; }
static __inline__ __attribute__((always_inline)) __m512i _mm512_castps_si512(__m512 a) { return (__m512i)a; }
static __inline__ __attribute__((always_inline)) __m512 _mm512_castpd_ps(__m512d a) { return (__m512)a; }
static __inline__ __attribute__((always_inline)) __m512i _mm512_castpd_si512(__m512d a) { return (__m512i)a; }
static __inline__ __attribute__((always_inline)) __m512 _mm512_castsi512_ps(__m512i a) { return (__m512)a; }
static __inline__ __attribute__((always_inline)) __m512d _mm512_castsi512_pd(__m512i a) { return (__m512d)a; }
#endif
