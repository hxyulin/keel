#ifndef _KEEL_SHAINTRIN_H
#define _KEEL_SHAINTRIN_H
#include <emmintrin.h>
typedef int __keel_sha_i32x4 __attribute__((vector_size(16)));
#define _mm_sha256msg1_epu32(a, b) ((__m128i)__builtin_ia32_sha256msg1((__keel_sha_i32x4)(a), (__keel_sha_i32x4)(b)))
#define _mm_sha256msg2_epu32(a, b) ((__m128i)__builtin_ia32_sha256msg2((__keel_sha_i32x4)(a), (__keel_sha_i32x4)(b)))
#define _mm_sha256rnds2_epu32(a, b, c) ((__m128i)__builtin_ia32_sha256rnds2((__keel_sha_i32x4)(a), (__keel_sha_i32x4)(b), (__keel_sha_i32x4)(c)))
#endif
