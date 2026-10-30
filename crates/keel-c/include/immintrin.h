/* Bounded Keel intrinsic subset; see docs/targets/x86_64/intrinsics.md. */
#ifndef _KEEL_IMMINTRIN_H
#define _KEEL_IMMINTRIN_H
#ifndef __x86_64__
#error "Keel x86 intrinsic headers require x86-64"
#endif
#include <avx2intrin.h>
#include <wmmintrin.h>
#include <shaintrin.h>
#include <fmaintrin.h>
#include <f16cintrin.h>

typedef long long __m512i __attribute__((vector_size(64), aligned(64)));
#define _mm256_clmulepi64_epi128(a,b,control) ((__m256i)__builtin_ia32_pclmulqdq256((__m256i)(a),(__m256i)(b),(control)))
#define _mm512_clmulepi64_epi128(a,b,control) ((__m512i)__builtin_ia32_pclmulqdq512((__m512i)(a),(__m512i)(b),(control)))

#endif /* _KEEL_IMMINTRIN_H */
