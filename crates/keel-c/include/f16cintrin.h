/* Keel F16C wrappers. See docs/targets/x86_64/intrinsics.md. */
#ifndef _KEEL_F16CINTRIN_H
#define _KEEL_F16CINTRIN_H
#include <avxintrin.h>
static __inline__ __m128 _mm_cvtph_ps(__m128i a) {
  return __builtin_keel_x86_ph_to_ps128(a[0]);
}
static __inline__ __m256 _mm256_cvtph_ps(__m128i a) {
  return __builtin_keel_x86_ph_to_ps256((__keel_i16_128)a);
}
#define _mm_cvtps_ph(a, i) ((__m128i){__builtin_keel_x86_ps_to_ph128((__m128)(a), (i)), 0})
#define _mm256_cvtps_ph(a, i) ((__m128i)__builtin_keel_x86_ps_to_ph256((__m256)(a), (i)))
static __inline__ float _cvtsh_ss(unsigned short a) {
  return _mm_cvtph_ps((__m128i){(long long)a, 0})[0];
}
#define _cvtss_sh(a, i) ((unsigned short)__builtin_keel_x86_ps_to_ph128((__m128){(a), 0, 0, 0}, (i)))
#endif /* _KEEL_F16CINTRIN_H */
