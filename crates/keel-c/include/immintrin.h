/* Bounded Keel intrinsic subset; see docs/c-x86-intrinsics.md. */
#ifndef _KEEL_IMMINTRIN_H
#define _KEEL_IMMINTRIN_H
#ifndef __x86_64__
#error "Keel x86 intrinsic headers require x86-64"
#endif
#include <avx2intrin.h>
#include <wmmintrin.h>
#include <fmaintrin.h>
#include <f16cintrin.h>

#endif /* _KEEL_IMMINTRIN_H */
