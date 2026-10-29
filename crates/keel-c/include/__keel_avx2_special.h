#ifndef _KEEL_AVX2_SPECIAL_H
#define _KEEL_AVX2_SPECIAL_H
#define _mm256_mpsadbw_epu8(a,b,i) ((__m256i)__builtin_keel_query_avx2vmpsadbw256((__keel_i8_256)(a),(__keel_i8_256)(b),(i)))
#endif /* _KEEL_AVX2_SPECIAL_H */
