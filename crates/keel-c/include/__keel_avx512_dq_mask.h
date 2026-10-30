/* AVX512DQ scalar masks and vector lane sign bits. Arguments evaluate once. */
#ifndef _KEEL_AVX512_DQ_MASK_H
#define _KEEL_AVX512_DQ_MASK_H
#define _mm_movepi32_mask(a) ((__mmask8)__builtin_keel_avx512dq_movd2m128((__keel_i32_128)(a)))
#define _mm_movepi64_mask(a) ((__mmask8)__builtin_keel_avx512dq_movq2m128((__m128i)(a)))
#define _mm_movm_epi32(k) ((__m128i)__builtin_keel_avx512dq_movm2d128((signed char)(k)))
#define _mm_movm_epi64(k) ((__m128i)__builtin_keel_avx512dq_movm2q128((signed char)(k)))
#define _mm256_movepi32_mask(a) ((__mmask8)__builtin_keel_avx512dq_movd2m256((__keel_i32_256)(a)))
#define _mm256_movepi64_mask(a) ((__mmask8)__builtin_keel_avx512dq_movq2m256((__m256i)(a)))
#define _mm256_movm_epi32(k) ((__m256i)__builtin_keel_avx512dq_movm2d256((signed char)(k)))
#define _mm256_movm_epi64(k) ((__m256i)__builtin_keel_avx512dq_movm2q256((signed char)(k)))
#define _mm512_movepi32_mask(a) ((__mmask16)__builtin_keel_avx512dq_movd2m512((__keel_avx512_i32x16)(a)))
#define _mm512_movepi64_mask(a) ((__mmask8)__builtin_keel_avx512dq_movq2m512((__m512i)(a)))
#define _mm512_movm_epi32(k) ((__m512i)__builtin_keel_avx512dq_movm2d512((short)(k)))
#define _mm512_movm_epi64(k) ((__m512i)__builtin_keel_avx512dq_movm2q512((signed char)(k)))
#endif
