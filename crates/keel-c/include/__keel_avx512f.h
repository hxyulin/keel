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
#endif
