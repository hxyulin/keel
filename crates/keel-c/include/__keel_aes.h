/* AES-owned fragment of the bounded wmmintrin.h contract. */
#ifndef _KEEL_AES_H
#define _KEEL_AES_H
#ifndef __x86_64__
#error "Keel AES intrinsic headers require x86-64"
#endif
#include <emmintrin.h>
/* Macros defer checking until use and evaluate each vector operand once. */
#define _mm_aesenc_si128(a, b) __builtin_ia32_aesenc128((a), (b))
#define _mm_aesenclast_si128(a, b) __builtin_ia32_aesenclast128((a), (b))
#define _mm_aesdec_si128(a, b) __builtin_ia32_aesdec128((a), (b))
#define _mm_aesdeclast_si128(a, b) __builtin_ia32_aesdeclast128((a), (b))
#define _mm_aesimc_si128(a) __builtin_ia32_aesimc128((a))
#define _mm_aeskeygenassist_si128(a, c) __builtin_ia32_aeskeygenassist128((a), (c))
#endif
