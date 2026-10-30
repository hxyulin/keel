/* Floating reductions use an explicit lower-first balanced tree. */
#ifndef _KEEL_AVX512_REDUCE_FLOAT_H
#define _KEEL_AVX512_REDUCE_FLOAT_H

/* At stride N/2, N/4, ... 1, combine lane i with lane i+stride, in that
   operand order. Only the live lower prefix executes arithmetic. Raw x86
   MIN/MAX choose the second operand for an equal or unordered pair. MXCSR
   rounding, denormal handling and exception flags remain observable; no SAE
   applies. Masked entry points substitute identities before any arithmetic. */

static __inline__ __attribute__((always_inline)) float
_mm512_reduce_add_ps(__m512 a) {
  a = _mm512_mask_add_ps((__m512){0}, 0xff, a, _mm512_shuffle_f32x4(a,a,0x4e));
  a = _mm512_mask_add_ps((__m512){0}, 0xf, a, _mm512_shuffle_f32x4(a,a,0xb1));
  a = _mm512_mask_add_ps((__m512){0}, 0x3, a, _mm512_permute_ps(a,0x4e));
  a = _mm512_mask_add_ps((__m512){0}, 0x1, a, _mm512_permute_ps(a,0xb1));
  return a[0];
}

static __inline__ __attribute__((always_inline)) float
_mm512_mask_reduce_add_ps(__mmask16 k, __m512 a) {
  a = _mm512_mask_mov_ps((__m512){0}, k, a);
  return _mm512_reduce_add_ps(a);
}

static __inline__ __attribute__((always_inline)) float
_mm512_reduce_mul_ps(__m512 a) {
  a = _mm512_mask_mul_ps((__m512){0}, 0xff, a, _mm512_shuffle_f32x4(a,a,0x4e));
  a = _mm512_mask_mul_ps((__m512){0}, 0xf, a, _mm512_shuffle_f32x4(a,a,0xb1));
  a = _mm512_mask_mul_ps((__m512){0}, 0x3, a, _mm512_permute_ps(a,0x4e));
  a = _mm512_mask_mul_ps((__m512){0}, 0x1, a, _mm512_permute_ps(a,0xb1));
  return a[0];
}

static __inline__ __attribute__((always_inline)) float
_mm512_mask_reduce_mul_ps(__mmask16 k, __m512 a) {
  a = _mm512_mask_mov_ps(_mm512_set1_ps(1.0f), k, a);
  return _mm512_reduce_mul_ps(a);
}

static __inline__ __attribute__((always_inline)) float
_mm512_reduce_min_ps(__m512 a) {
  a = _mm512_mask_min_ps((__m512){0}, 0xff, a, _mm512_shuffle_f32x4(a,a,0x4e));
  a = _mm512_mask_min_ps((__m512){0}, 0xf, a, _mm512_shuffle_f32x4(a,a,0xb1));
  a = _mm512_mask_min_ps((__m512){0}, 0x3, a, _mm512_permute_ps(a,0x4e));
  a = _mm512_mask_min_ps((__m512){0}, 0x1, a, _mm512_permute_ps(a,0xb1));
  return a[0];
}

static __inline__ __attribute__((always_inline)) float
_mm512_mask_reduce_min_ps(__mmask16 k, __m512 a) {
  a = _mm512_mask_mov_ps(_mm512_set1_ps(__builtin_inff()), k, a);
  return _mm512_reduce_min_ps(a);
}

static __inline__ __attribute__((always_inline)) float
_mm512_reduce_max_ps(__m512 a) {
  a = _mm512_mask_max_ps((__m512){0}, 0xff, a, _mm512_shuffle_f32x4(a,a,0x4e));
  a = _mm512_mask_max_ps((__m512){0}, 0xf, a, _mm512_shuffle_f32x4(a,a,0xb1));
  a = _mm512_mask_max_ps((__m512){0}, 0x3, a, _mm512_permute_ps(a,0x4e));
  a = _mm512_mask_max_ps((__m512){0}, 0x1, a, _mm512_permute_ps(a,0xb1));
  return a[0];
}

static __inline__ __attribute__((always_inline)) float
_mm512_mask_reduce_max_ps(__mmask16 k, __m512 a) {
  a = _mm512_mask_mov_ps(_mm512_set1_ps(-__builtin_inff()), k, a);
  return _mm512_reduce_max_ps(a);
}

static __inline__ __attribute__((always_inline)) double
_mm512_reduce_add_pd(__m512d a) {
  a = _mm512_mask_add_pd((__m512d){0}, 0xf, a, _mm512_shuffle_f64x2(a,a,0x4e));
  a = _mm512_mask_add_pd((__m512d){0}, 0x3, a, _mm512_shuffle_f64x2(a,a,0xb1));
  a = _mm512_mask_add_pd((__m512d){0}, 0x1, a, _mm512_permute_pd(a,0x55));
  return a[0];
}

static __inline__ __attribute__((always_inline)) double
_mm512_mask_reduce_add_pd(__mmask8 k, __m512d a) {
  a = _mm512_mask_mov_pd((__m512d){0}, k, a);
  return _mm512_reduce_add_pd(a);
}

static __inline__ __attribute__((always_inline)) double
_mm512_reduce_mul_pd(__m512d a) {
  a = _mm512_mask_mul_pd((__m512d){0}, 0xf, a, _mm512_shuffle_f64x2(a,a,0x4e));
  a = _mm512_mask_mul_pd((__m512d){0}, 0x3, a, _mm512_shuffle_f64x2(a,a,0xb1));
  a = _mm512_mask_mul_pd((__m512d){0}, 0x1, a, _mm512_permute_pd(a,0x55));
  return a[0];
}

static __inline__ __attribute__((always_inline)) double
_mm512_mask_reduce_mul_pd(__mmask8 k, __m512d a) {
  a = _mm512_mask_mov_pd(_mm512_set1_pd(1.0), k, a);
  return _mm512_reduce_mul_pd(a);
}

static __inline__ __attribute__((always_inline)) double
_mm512_reduce_min_pd(__m512d a) {
  a = _mm512_mask_min_pd((__m512d){0}, 0xf, a, _mm512_shuffle_f64x2(a,a,0x4e));
  a = _mm512_mask_min_pd((__m512d){0}, 0x3, a, _mm512_shuffle_f64x2(a,a,0xb1));
  a = _mm512_mask_min_pd((__m512d){0}, 0x1, a, _mm512_permute_pd(a,0x55));
  return a[0];
}

static __inline__ __attribute__((always_inline)) double
_mm512_mask_reduce_min_pd(__mmask8 k, __m512d a) {
  a = _mm512_mask_mov_pd(_mm512_set1_pd(__builtin_inf()), k, a);
  return _mm512_reduce_min_pd(a);
}

static __inline__ __attribute__((always_inline)) double
_mm512_reduce_max_pd(__m512d a) {
  a = _mm512_mask_max_pd((__m512d){0}, 0xf, a, _mm512_shuffle_f64x2(a,a,0x4e));
  a = _mm512_mask_max_pd((__m512d){0}, 0x3, a, _mm512_shuffle_f64x2(a,a,0xb1));
  a = _mm512_mask_max_pd((__m512d){0}, 0x1, a, _mm512_permute_pd(a,0x55));
  return a[0];
}

static __inline__ __attribute__((always_inline)) double
_mm512_mask_reduce_max_pd(__mmask8 k, __m512d a) {
  a = _mm512_mask_mov_pd(_mm512_set1_pd(-__builtin_inf()), k, a);
  return _mm512_reduce_max_pd(a);
}

#endif
