/* Comparisons retain quiet/signaling MXCSR effects, even for unused results. */
#ifndef _KEEL_SSE_COMPARE_H
#define _KEEL_SSE_COMPARE_H
typedef int __keel_cmp_i32_128 __attribute__((vector_size(16)));

static __inline__ __attribute__((always_inline)) __m128 _mm_cmpeq_ps(__m128 a, __m128 b) {
  return (__m128)__builtin_keel_sse_cmpps128(a, b, 0);
}
static __inline__ __attribute__((always_inline)) __m128 _mm_cmplt_ps(__m128 a, __m128 b) {
  return (__m128)__builtin_keel_sse_cmpps128(a, b, 1);
}
static __inline__ __attribute__((always_inline)) __m128 _mm_cmple_ps(__m128 a, __m128 b) {
  return (__m128)__builtin_keel_sse_cmpps128(a, b, 2);
}
static __inline__ __attribute__((always_inline)) __m128 _mm_cmpunord_ps(__m128 a, __m128 b) {
  return (__m128)__builtin_keel_sse_cmpps128(a, b, 3);
}
static __inline__ __attribute__((always_inline)) __m128 _mm_cmpneq_ps(__m128 a, __m128 b) {
  return (__m128)__builtin_keel_sse_cmpps128(a, b, 4);
}
static __inline__ __attribute__((always_inline)) __m128 _mm_cmpnlt_ps(__m128 a, __m128 b) {
  return (__m128)__builtin_keel_sse_cmpps128(a, b, 5);
}
static __inline__ __attribute__((always_inline)) __m128 _mm_cmpnle_ps(__m128 a, __m128 b) {
  return (__m128)__builtin_keel_sse_cmpps128(a, b, 6);
}
static __inline__ __attribute__((always_inline)) __m128 _mm_cmpord_ps(__m128 a, __m128 b) {
  return (__m128)__builtin_keel_sse_cmpps128(a, b, 7);
}
static __inline__ __attribute__((always_inline)) __m128 _mm_cmpgt_ps(__m128 a, __m128 b) {
  return (__m128)__builtin_keel_sse_cmpps128(b, a, 1);
}
static __inline__ __attribute__((always_inline)) __m128 _mm_cmpge_ps(__m128 a, __m128 b) {
  return (__m128)__builtin_keel_sse_cmpps128(b, a, 2);
}
static __inline__ __attribute__((always_inline)) __m128 _mm_cmpngt_ps(__m128 a, __m128 b) {
  return (__m128)__builtin_keel_sse_cmpps128(b, a, 5);
}
static __inline__ __attribute__((always_inline)) __m128 _mm_cmpnge_ps(__m128 a, __m128 b) {
  return (__m128)__builtin_keel_sse_cmpps128(b, a, 6);
}
static __inline__ __attribute__((always_inline)) __m128 _mm_cmpeq_ss(__m128 a, __m128 b) {
  __keel_cmp_i32_128 result = (__keel_cmp_i32_128)a;
  result[0] = __builtin_keel_sse_cmpss(a[0], b[0], 0);
  return (__m128)result;
}
static __inline__ __attribute__((always_inline)) __m128 _mm_cmplt_ss(__m128 a, __m128 b) {
  __keel_cmp_i32_128 result = (__keel_cmp_i32_128)a;
  result[0] = __builtin_keel_sse_cmpss(a[0], b[0], 1);
  return (__m128)result;
}
static __inline__ __attribute__((always_inline)) __m128 _mm_cmple_ss(__m128 a, __m128 b) {
  __keel_cmp_i32_128 result = (__keel_cmp_i32_128)a;
  result[0] = __builtin_keel_sse_cmpss(a[0], b[0], 2);
  return (__m128)result;
}
static __inline__ __attribute__((always_inline)) __m128 _mm_cmpunord_ss(__m128 a, __m128 b) {
  __keel_cmp_i32_128 result = (__keel_cmp_i32_128)a;
  result[0] = __builtin_keel_sse_cmpss(a[0], b[0], 3);
  return (__m128)result;
}
static __inline__ __attribute__((always_inline)) __m128 _mm_cmpneq_ss(__m128 a, __m128 b) {
  __keel_cmp_i32_128 result = (__keel_cmp_i32_128)a;
  result[0] = __builtin_keel_sse_cmpss(a[0], b[0], 4);
  return (__m128)result;
}
static __inline__ __attribute__((always_inline)) __m128 _mm_cmpnlt_ss(__m128 a, __m128 b) {
  __keel_cmp_i32_128 result = (__keel_cmp_i32_128)a;
  result[0] = __builtin_keel_sse_cmpss(a[0], b[0], 5);
  return (__m128)result;
}
static __inline__ __attribute__((always_inline)) __m128 _mm_cmpnle_ss(__m128 a, __m128 b) {
  __keel_cmp_i32_128 result = (__keel_cmp_i32_128)a;
  result[0] = __builtin_keel_sse_cmpss(a[0], b[0], 6);
  return (__m128)result;
}
static __inline__ __attribute__((always_inline)) __m128 _mm_cmpord_ss(__m128 a, __m128 b) {
  __keel_cmp_i32_128 result = (__keel_cmp_i32_128)a;
  result[0] = __builtin_keel_sse_cmpss(a[0], b[0], 7);
  return (__m128)result;
}
static __inline__ __attribute__((always_inline)) __m128 _mm_cmpgt_ss(__m128 a, __m128 b) {
  __keel_cmp_i32_128 result = (__keel_cmp_i32_128)a;
  result[0] = __builtin_keel_sse_cmpss(b[0], a[0], 1);
  return (__m128)result;
}
static __inline__ __attribute__((always_inline)) __m128 _mm_cmpge_ss(__m128 a, __m128 b) {
  __keel_cmp_i32_128 result = (__keel_cmp_i32_128)a;
  result[0] = __builtin_keel_sse_cmpss(b[0], a[0], 2);
  return (__m128)result;
}
static __inline__ __attribute__((always_inline)) __m128 _mm_cmpngt_ss(__m128 a, __m128 b) {
  __keel_cmp_i32_128 result = (__keel_cmp_i32_128)a;
  result[0] = __builtin_keel_sse_cmpss(b[0], a[0], 5);
  return (__m128)result;
}
static __inline__ __attribute__((always_inline)) __m128 _mm_cmpnge_ss(__m128 a, __m128 b) {
  __keel_cmp_i32_128 result = (__keel_cmp_i32_128)a;
  result[0] = __builtin_keel_sse_cmpss(b[0], a[0], 6);
  return (__m128)result;
}
static __inline__ __attribute__((always_inline)) int _mm_comieq_ss(__m128 a, __m128 b) {
  int flags = __builtin_keel_sse_comiss(a[0], b[0]);
  return (flags & 0x44) == 0x40;
}
static __inline__ __attribute__((always_inline)) int _mm_comilt_ss(__m128 a, __m128 b) {
  int flags = __builtin_keel_sse_comiss(a[0], b[0]);
  return (flags & 5) == 1;
}
static __inline__ __attribute__((always_inline)) int _mm_comile_ss(__m128 a, __m128 b) {
  int flags = __builtin_keel_sse_comiss(a[0], b[0]);
  return (flags & 4) == 0 && (flags & 0x41) != 0;
}
static __inline__ __attribute__((always_inline)) int _mm_comigt_ss(__m128 a, __m128 b) {
  int flags = __builtin_keel_sse_comiss(a[0], b[0]);
  return (flags & 0x41) == 0;
}
static __inline__ __attribute__((always_inline)) int _mm_comige_ss(__m128 a, __m128 b) {
  int flags = __builtin_keel_sse_comiss(a[0], b[0]);
  return (flags & 1) == 0;
}
static __inline__ __attribute__((always_inline)) int _mm_comineq_ss(__m128 a, __m128 b) {
  int flags = __builtin_keel_sse_comiss(a[0], b[0]);
  return (flags & 0x40) == 0 || (flags & 4) != 0;
}
static __inline__ __attribute__((always_inline)) int _mm_ucomieq_ss(__m128 a, __m128 b) {
  int flags = __builtin_keel_sse_ucomiss(a[0], b[0]);
  return (flags & 0x44) == 0x40;
}
static __inline__ __attribute__((always_inline)) int _mm_ucomilt_ss(__m128 a, __m128 b) {
  int flags = __builtin_keel_sse_ucomiss(a[0], b[0]);
  return (flags & 5) == 1;
}
static __inline__ __attribute__((always_inline)) int _mm_ucomile_ss(__m128 a, __m128 b) {
  int flags = __builtin_keel_sse_ucomiss(a[0], b[0]);
  return (flags & 4) == 0 && (flags & 0x41) != 0;
}
static __inline__ __attribute__((always_inline)) int _mm_ucomigt_ss(__m128 a, __m128 b) {
  int flags = __builtin_keel_sse_ucomiss(a[0], b[0]);
  return (flags & 0x41) == 0;
}
static __inline__ __attribute__((always_inline)) int _mm_ucomige_ss(__m128 a, __m128 b) {
  int flags = __builtin_keel_sse_ucomiss(a[0], b[0]);
  return (flags & 1) == 0;
}
static __inline__ __attribute__((always_inline)) int _mm_ucomineq_ss(__m128 a, __m128 b) {
  int flags = __builtin_keel_sse_ucomiss(a[0], b[0]);
  return (flags & 0x40) == 0 || (flags & 4) != 0;
}

#endif /* _KEEL_SSE_COMPARE_H */
