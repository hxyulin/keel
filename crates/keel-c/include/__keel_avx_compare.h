/* All 32 AVX predicates preserve architectural result and MXCSR semantics. */
#ifndef _KEEL_AVX_COMPARE_H
#define _KEEL_AVX_COMPARE_H

#define _CMP_EQ_OQ 0
#define _CMP_LT_OS 1
#define _CMP_LE_OS 2
#define _CMP_UNORD_Q 3
#define _CMP_NEQ_UQ 4
#define _CMP_NLT_US 5
#define _CMP_NLE_US 6
#define _CMP_ORD_Q 7
#define _CMP_EQ_UQ 8
#define _CMP_NGE_US 9
#define _CMP_NGT_US 10
#define _CMP_FALSE_OQ 11
#define _CMP_NEQ_OQ 12
#define _CMP_GE_OS 13
#define _CMP_GT_OS 14
#define _CMP_TRUE_UQ 15
#define _CMP_EQ_OS 16
#define _CMP_LT_OQ 17
#define _CMP_LE_OQ 18
#define _CMP_UNORD_S 19
#define _CMP_NEQ_US 20
#define _CMP_NLT_UQ 21
#define _CMP_NLE_UQ 22
#define _CMP_ORD_S 23
#define _CMP_EQ_US 24
#define _CMP_NGE_UQ 25
#define _CMP_NGT_UQ 26
#define _CMP_FALSE_OS 27
#define _CMP_NEQ_OS 28
#define _CMP_GE_OQ 29
#define _CMP_GT_OQ 30
#define _CMP_TRUE_US 31
#define _mm_cmp_ps(a,b,predicate) ((__m128)__builtin_keel_avx_vcmpps128((a),(b),(predicate)))
#define _mm256_cmp_ps(a,b,predicate) ((__m256)__builtin_keel_avx_vcmpps256((a),(b),(predicate)))
#define _mm_cmp_pd(a,b,predicate) ((__m128d)__builtin_keel_avx_vcmppd128((a),(b),(predicate)))
#define _mm256_cmp_pd(a,b,predicate) ((__m256d)__builtin_keel_avx_vcmppd256((a),(b),(predicate)))
#define _mm_cmp_ss(a,b,predicate) \
  __extension__({ \
    __m128 __keel_cmp_a=(a), __keel_cmp_b=(b); \
    __keel_cmp_i32_128 __keel_cmp_result=(__keel_cmp_i32_128)__keel_cmp_a; \
    __keel_cmp_result[0]=__builtin_keel_avx_vcmpss(__keel_cmp_a[0],__keel_cmp_b[0],(predicate)); \
    (__m128)__keel_cmp_result; \
  })
#define _mm_cmp_sd(a,b,predicate) \
  __extension__({ \
    __m128d __keel_cmp_a=(a), __keel_cmp_b=(b); \
    __keel_cmp_i64_128 __keel_cmp_result=(__keel_cmp_i64_128)__keel_cmp_a; \
    __keel_cmp_result[0]=__builtin_keel_avx_vcmpsd(__keel_cmp_a[0],__keel_cmp_b[0],(predicate)); \
    (__m128d)__keel_cmp_result; \
  })

#endif /* _KEEL_AVX_COMPARE_H */
