/* Bounded Keel PCLMULQDQ and AES-NI subset; no full vendor header contract. */
#ifndef _KEEL_WMMINTRIN_H
#define _KEEL_WMMINTRIN_H
#ifndef __x86_64__
#error "Keel crypto intrinsic header requires x86-64"
#endif
#include <emmintrin.h>
/* Macro retains the ICE control and evaluates each vector operand once. */
#define _mm_clmulepi64_si128(a, b, control) \
  ((__m128i)__builtin_ia32_pclmulqdq128((__m128i)(a), (__m128i)(b), (control)))
#include <__keel_aes.h>
#endif
