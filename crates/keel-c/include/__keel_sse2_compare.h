/* Comparisons retain quiet/signaling MXCSR effects, even for unused results. */
#ifndef _KEEL_SSE2_COMPARE_H
#define _KEEL_SSE2_COMPARE_H
typedef long long __keel_cmp_i64_128 __attribute__((vector_size(16)));

static __inline__ __attribute__((always_inline)) __m128d _mm_cmpeq_pd(__m128d a, __m128d b) {
  return (__m128d)__builtin_keel_sse_cmppd128(a, b, 0);
}
static __inline__ __attribute__((always_inline)) __m128d _mm_cmplt_pd(__m128d a, __m128d b) {
  return (__m128d)__builtin_keel_sse_cmppd128(a, b, 1);
}
static __inline__ __attribute__((always_inline)) __m128d _mm_cmple_pd(__m128d a, __m128d b) {
  return (__m128d)__builtin_keel_sse_cmppd128(a, b, 2);
}
static __inline__ __attribute__((always_inline)) __m128d _mm_cmpunord_pd(__m128d a, __m128d b) {
  return (__m128d)__builtin_keel_sse_cmppd128(a, b, 3);
}
static __inline__ __attribute__((always_inline)) __m128d _mm_cmpneq_pd(__m128d a, __m128d b) {
  return (__m128d)__builtin_keel_sse_cmppd128(a, b, 4);
}
static __inline__ __attribute__((always_inline)) __m128d _mm_cmpnlt_pd(__m128d a, __m128d b) {
  return (__m128d)__builtin_keel_sse_cmppd128(a, b, 5);
}
static __inline__ __attribute__((always_inline)) __m128d _mm_cmpnle_pd(__m128d a, __m128d b) {
  return (__m128d)__builtin_keel_sse_cmppd128(a, b, 6);
}
static __inline__ __attribute__((always_inline)) __m128d _mm_cmpord_pd(__m128d a, __m128d b) {
  return (__m128d)__builtin_keel_sse_cmppd128(a, b, 7);
}
static __inline__ __attribute__((always_inline)) __m128d _mm_cmpgt_pd(__m128d a, __m128d b) {
  return (__m128d)__builtin_keel_sse_cmppd128(b, a, 1);
}
static __inline__ __attribute__((always_inline)) __m128d _mm_cmpge_pd(__m128d a, __m128d b) {
  return (__m128d)__builtin_keel_sse_cmppd128(b, a, 2);
}
static __inline__ __attribute__((always_inline)) __m128d _mm_cmpngt_pd(__m128d a, __m128d b) {
  return (__m128d)__builtin_keel_sse_cmppd128(b, a, 5);
}
static __inline__ __attribute__((always_inline)) __m128d _mm_cmpnge_pd(__m128d a, __m128d b) {
  return (__m128d)__builtin_keel_sse_cmppd128(b, a, 6);
}
static __inline__ __attribute__((always_inline)) __m128d _mm_cmpeq_sd(__m128d a, __m128d b) {
  __keel_cmp_i64_128 result = (__keel_cmp_i64_128)a;
  result[0] = __builtin_keel_sse_cmpsd(a[0], b[0], 0);
  return (__m128d)result;
}
static __inline__ __attribute__((always_inline)) __m128d _mm_cmplt_sd(__m128d a, __m128d b) {
  __keel_cmp_i64_128 result = (__keel_cmp_i64_128)a;
  result[0] = __builtin_keel_sse_cmpsd(a[0], b[0], 1);
  return (__m128d)result;
}
static __inline__ __attribute__((always_inline)) __m128d _mm_cmple_sd(__m128d a, __m128d b) {
  __keel_cmp_i64_128 result = (__keel_cmp_i64_128)a;
  result[0] = __builtin_keel_sse_cmpsd(a[0], b[0], 2);
  return (__m128d)result;
}
static __inline__ __attribute__((always_inline)) __m128d _mm_cmpunord_sd(__m128d a, __m128d b) {
  __keel_cmp_i64_128 result = (__keel_cmp_i64_128)a;
  result[0] = __builtin_keel_sse_cmpsd(a[0], b[0], 3);
  return (__m128d)result;
}
static __inline__ __attribute__((always_inline)) __m128d _mm_cmpneq_sd(__m128d a, __m128d b) {
  __keel_cmp_i64_128 result = (__keel_cmp_i64_128)a;
  result[0] = __builtin_keel_sse_cmpsd(a[0], b[0], 4);
  return (__m128d)result;
}
static __inline__ __attribute__((always_inline)) __m128d _mm_cmpnlt_sd(__m128d a, __m128d b) {
  __keel_cmp_i64_128 result = (__keel_cmp_i64_128)a;
  result[0] = __builtin_keel_sse_cmpsd(a[0], b[0], 5);
  return (__m128d)result;
}
static __inline__ __attribute__((always_inline)) __m128d _mm_cmpnle_sd(__m128d a, __m128d b) {
  __keel_cmp_i64_128 result = (__keel_cmp_i64_128)a;
  result[0] = __builtin_keel_sse_cmpsd(a[0], b[0], 6);
  return (__m128d)result;
}
static __inline__ __attribute__((always_inline)) __m128d _mm_cmpord_sd(__m128d a, __m128d b) {
  __keel_cmp_i64_128 result = (__keel_cmp_i64_128)a;
  result[0] = __builtin_keel_sse_cmpsd(a[0], b[0], 7);
  return (__m128d)result;
}
static __inline__ __attribute__((always_inline)) __m128d _mm_cmpgt_sd(__m128d a, __m128d b) {
  __keel_cmp_i64_128 result = (__keel_cmp_i64_128)a;
  result[0] = __builtin_keel_sse_cmpsd(b[0], a[0], 1);
  return (__m128d)result;
}
static __inline__ __attribute__((always_inline)) __m128d _mm_cmpge_sd(__m128d a, __m128d b) {
  __keel_cmp_i64_128 result = (__keel_cmp_i64_128)a;
  result[0] = __builtin_keel_sse_cmpsd(b[0], a[0], 2);
  return (__m128d)result;
}
static __inline__ __attribute__((always_inline)) __m128d _mm_cmpngt_sd(__m128d a, __m128d b) {
  __keel_cmp_i64_128 result = (__keel_cmp_i64_128)a;
  result[0] = __builtin_keel_sse_cmpsd(b[0], a[0], 5);
  return (__m128d)result;
}
static __inline__ __attribute__((always_inline)) __m128d _mm_cmpnge_sd(__m128d a, __m128d b) {
  __keel_cmp_i64_128 result = (__keel_cmp_i64_128)a;
  result[0] = __builtin_keel_sse_cmpsd(b[0], a[0], 6);
  return (__m128d)result;
}
static __inline__ __attribute__((always_inline)) int _mm_comieq_sd(__m128d a, __m128d b) {
  int flags = __builtin_keel_sse_comisd(a[0], b[0]);
  return (flags & 0x44) == 0x40;
}
static __inline__ __attribute__((always_inline)) int _mm_comilt_sd(__m128d a, __m128d b) {
  int flags = __builtin_keel_sse_comisd(a[0], b[0]);
  return (flags & 5) == 1;
}
static __inline__ __attribute__((always_inline)) int _mm_comile_sd(__m128d a, __m128d b) {
  int flags = __builtin_keel_sse_comisd(a[0], b[0]);
  return (flags & 4) == 0 && (flags & 0x41) != 0;
}
static __inline__ __attribute__((always_inline)) int _mm_comigt_sd(__m128d a, __m128d b) {
  int flags = __builtin_keel_sse_comisd(a[0], b[0]);
  return (flags & 0x41) == 0;
}
static __inline__ __attribute__((always_inline)) int _mm_comige_sd(__m128d a, __m128d b) {
  int flags = __builtin_keel_sse_comisd(a[0], b[0]);
  return (flags & 1) == 0;
}
static __inline__ __attribute__((always_inline)) int _mm_comineq_sd(__m128d a, __m128d b) {
  int flags = __builtin_keel_sse_comisd(a[0], b[0]);
  return (flags & 0x40) == 0 || (flags & 4) != 0;
}
static __inline__ __attribute__((always_inline)) int _mm_ucomieq_sd(__m128d a, __m128d b) {
  int flags = __builtin_keel_sse_ucomisd(a[0], b[0]);
  return (flags & 0x44) == 0x40;
}
static __inline__ __attribute__((always_inline)) int _mm_ucomilt_sd(__m128d a, __m128d b) {
  int flags = __builtin_keel_sse_ucomisd(a[0], b[0]);
  return (flags & 5) == 1;
}
static __inline__ __attribute__((always_inline)) int _mm_ucomile_sd(__m128d a, __m128d b) {
  int flags = __builtin_keel_sse_ucomisd(a[0], b[0]);
  return (flags & 4) == 0 && (flags & 0x41) != 0;
}
static __inline__ __attribute__((always_inline)) int _mm_ucomigt_sd(__m128d a, __m128d b) {
  int flags = __builtin_keel_sse_ucomisd(a[0], b[0]);
  return (flags & 0x41) == 0;
}
static __inline__ __attribute__((always_inline)) int _mm_ucomige_sd(__m128d a, __m128d b) {
  int flags = __builtin_keel_sse_ucomisd(a[0], b[0]);
  return (flags & 1) == 0;
}
static __inline__ __attribute__((always_inline)) int _mm_ucomineq_sd(__m128d a, __m128d b) {
  int flags = __builtin_keel_sse_ucomisd(a[0], b[0]);
  return (flags & 0x40) == 0 || (flags & 4) != 0;
}

#endif /* _KEEL_SSE2_COMPARE_H */
