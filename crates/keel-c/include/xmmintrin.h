/* Bounded Keel intrinsic subset; see docs/c-x86-intrinsics.md. */
#ifndef _KEEL_XMMINTRIN_H
#define _KEEL_XMMINTRIN_H
#ifndef __x86_64__
#error "Keel x86 intrinsic headers require x86-64"
#endif
typedef float __m128 __attribute__((vector_size(16)));
static __inline__ __m128 _mm_add_ps(__m128 a, __m128 b) { return a + b; }
static __inline__ __m128 _mm_sub_ps(__m128 a, __m128 b) { return a - b; }
static __inline__ __m128 _mm_mul_ps(__m128 a, __m128 b) { return a * b; }
static __inline__ __m128 _mm_div_ps(__m128 a, __m128 b) { return a / b; }
static __inline__ __m128 _mm_cmpeq_ps(__m128 a, __m128 b) {
  return (__m128)(a == b);
}
static __inline__ __m128 _mm_cmplt_ps(__m128 a, __m128 b) {
  return (__m128)(a < b);
}
static __inline__ __m128 _mm_cmple_ps(__m128 a, __m128 b) {
  return (__m128)(a <= b);
}
static __inline__ __m128 _mm_cmpgt_ps(__m128 a, __m128 b) {
  return (__m128)(a > b);
}
static __inline__ __m128 _mm_cmpge_ps(__m128 a, __m128 b) {
  return (__m128)(a >= b);
}
static __inline__ __m128 _mm_cmpneq_ps(__m128 a, __m128 b) {
  return (__m128)(a != b);
}
static __inline__ __m128 _mm_setzero_ps(void) { return (__m128){0}; }
static __inline__ __m128 _mm_set1_ps(float x) { return (__m128){x, x, x, x}; }
static __inline__ __m128 _mm_set_ps(float x3, float x2, float x1, float x0) {
  return (__m128){x0, x1, x2, x3};
}
static __inline__ __m128 _mm_load_ps(const float *p) {
  __m128 r;
  __builtin_memcpy(&r, p, 16);
  return r;
}
static __inline__ void _mm_store_ps(float *p, __m128 a) {
  __builtin_memcpy(p, &a, 16);
}
static __inline__ __m128 _mm_loadu_ps(const float *p) {
  __m128 r;
  __builtin_memcpy(&r, p, 16);
  return r;
}
static __inline__ void _mm_storeu_ps(float *p, __m128 a) {
  __builtin_memcpy(p, &a, 16);
}
#define _MM_SHUFFLE(z, y, x, w) (((z) << 6) | ((y) << 4) | ((x) << 2) | (w))
#define _mm_shuffle_ps(a, b, i)                                                \
  __builtin_shufflevector((a), (b), (i) & 3, ((i) >> 2) & 3,                   \
                          4 + (((i) >> 4) & 3), 4 + (((i) >> 6) & 3))
static __inline__ float _mm_cvtss_f32(__m128 a) { return a[0]; }

typedef unsigned long long __keel_ps_bits __attribute__((vector_size(16)));
static __inline__ __m128 _mm_and_ps(__m128 a, __m128 b) {
  return (__m128)((__keel_ps_bits)a & (__keel_ps_bits)b);
}
static __inline__ __m128 _mm_or_ps(__m128 a, __m128 b) {
  return (__m128)((__keel_ps_bits)a | (__keel_ps_bits)b);
}
static __inline__ __m128 _mm_xor_ps(__m128 a, __m128 b) {
  return (__m128)((__keel_ps_bits)a ^ (__keel_ps_bits)b);
}
static __inline__ __m128 _mm_andnot_ps(__m128 a, __m128 b) {
  return (__m128)(~(__keel_ps_bits)a & (__keel_ps_bits)b);
}

static __inline__ __m128 _mm_unpacklo_ps(__m128 a, __m128 b) {
  return (__m128)__builtin_shufflevector(a, b, 0, 4, 1, 5);
}
static __inline__ __m128 _mm_unpackhi_ps(__m128 a, __m128 b) {
  return (__m128)__builtin_shufflevector(a, b, 2, 6, 3, 7);
}

#endif /* _KEEL_XMMINTRIN_H */
