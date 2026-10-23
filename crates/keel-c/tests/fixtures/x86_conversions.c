#include <emmintrin.h>
_Static_assert(sizeof(__m128)==16 && _Alignof(__m128)==16, "float layout");
_Static_assert(sizeof(__m128i)==16 && _Alignof(__m128i)==16, "integer layout");
_Static_assert(__builtin_types_compatible_p(__typeof__(&_mm_cvtepi32_ps), __m128 (*)(__m128i)), "i2f prototype");
_Static_assert(__builtin_types_compatible_p(__typeof__(&_mm_cvttps_epi32), __m128i (*)(__m128)), "trunc prototype");
_Static_assert(__builtin_types_compatible_p(__typeof__(&_mm_cvtps_epi32), __m128i (*)(__m128)), "round prototype");
int k_evaluations;
static __m128i k_read(const void *p) {
  k_evaluations++;
  return _mm_loadu_si128(p);
}
__m128 k_i2f(__m128i a) { return _mm_cvtepi32_ps(a); }
__m128i k_trunc(__m128 a) { return _mm_cvttps_epi32(a); }
__m128i k_round(__m128 a) { return _mm_cvtps_epi32(a); }
void k_mem0(void *out, const void *in) {
  _mm_storeu_ps(out, _mm_cvtepi32_ps(k_read(in)));
}
void k_mem1(void *out, const void *in) {
  _mm_storeu_si128(out, _mm_cvttps_epi32(_mm_castsi128_ps(k_read(in))));
}
void k_mem2(void *out, const void *in) {
  _mm_storeu_si128(out, _mm_cvtps_epi32(_mm_castsi128_ps(k_read(in))));
}
void k_dead0(__m128i a) { _mm_cvtepi32_ps(a); }
void k_dead1(__m128 a) { _mm_cvttps_epi32(a); }
void k_dead2(__m128 a) { _mm_cvtps_epi32(a); }
extern __m128i k_callback(__m128i);
__m128 k_direct(__m128i a) { return _mm_cvtepi32_ps(k_callback(a)); }
__m128i k_indirect(__m128 a, __m128(*f)(__m128)) {
  return _mm_cvtps_epi32(f(a));
}
/* Both original input and conversion result remain live across a call. */
extern void k_capture(__m128i, __m128);
void k_live(__m128i a) { __m128 b = _mm_cvtepi32_ps(a); k_capture(a,b); }
/* An external call can observe/change MXCSR between identical conversions. */
extern void k_change(void);
void k_sequence(void *a, void *b, __m128i x) {
  _mm_storeu_ps(a, _mm_cvtepi32_ps(x));
  k_change();
  _mm_storeu_ps(b, _mm_cvtepi32_ps(x));
}
