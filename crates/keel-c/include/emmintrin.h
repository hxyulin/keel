/* Bounded Keel intrinsic subset; see docs/c-x86-intrinsics.md. */
#ifndef _KEEL_EMMINTRIN_H
#define _KEEL_EMMINTRIN_H
#ifndef __x86_64__
#error "Keel x86 intrinsic headers require x86-64"
#endif
#include <xmmintrin.h>
typedef double __m128d __attribute__((vector_size(16)));
static __inline__ __m128d _mm_add_pd(__m128d a, __m128d b) { return a + b; }
static __inline__ __m128d _mm_sub_pd(__m128d a, __m128d b) { return a - b; }
static __inline__ __m128d _mm_mul_pd(__m128d a, __m128d b) { return a * b; }
static __inline__ __m128d _mm_div_pd(__m128d a, __m128d b) { return a / b; }
static __inline__ __m128d _mm_cmpeq_pd(__m128d a, __m128d b) {
  return (__m128d)(a == b);
}
static __inline__ __m128d _mm_cmplt_pd(__m128d a, __m128d b) {
  return (__m128d)(a < b);
}
static __inline__ __m128d _mm_cmple_pd(__m128d a, __m128d b) {
  return (__m128d)(a <= b);
}
static __inline__ __m128d _mm_cmpgt_pd(__m128d a, __m128d b) {
  return (__m128d)(a > b);
}
static __inline__ __m128d _mm_cmpge_pd(__m128d a, __m128d b) {
  return (__m128d)(a >= b);
}
static __inline__ __m128d _mm_cmpneq_pd(__m128d a, __m128d b) {
  return (__m128d)(a != b);
}
static __inline__ __m128d _mm_setzero_pd(void) { return (__m128d){0}; }
static __inline__ __m128d _mm_set1_pd(double x) { return (__m128d){x, x}; }
static __inline__ __m128d _mm_set_pd(double x1, double x0) {
  return (__m128d){x0, x1};
}
static __inline__ __m128d _mm_load_pd(const double *p) {
  __m128d r;
  __builtin_memcpy(&r, p, 16);
  return r;
}
static __inline__ void _mm_store_pd(double *p, __m128d a) {
  __builtin_memcpy(p, &a, 16);
}
static __inline__ __m128d _mm_loadu_pd(const double *p) {
  __m128d r;
  __builtin_memcpy(&r, p, 16);
  return r;
}
static __inline__ void _mm_storeu_pd(double *p, __m128d a) {
  __builtin_memcpy(p, &a, 16);
}
typedef long long __m128i __attribute__((vector_size(16)));
typedef unsigned char __keel_u8_128 __attribute__((vector_size(16)));
typedef signed char __keel_i8_128 __attribute__((vector_size(16)));
static __inline__ __m128i _mm_add_epi8(__m128i a, __m128i b) {
  return (__m128i)((__keel_u8_128)a + (__keel_u8_128)b);
}
static __inline__ __m128i _mm_sub_epi8(__m128i a, __m128i b) {
  return (__m128i)((__keel_u8_128)a - (__keel_u8_128)b);
}
static __inline__ __m128i _mm_cmpeq_epi8(__m128i a, __m128i b) {
  return (__m128i)((__keel_i8_128)a == (__keel_i8_128)b);
}
static __inline__ __m128i _mm_cmpgt_epi8(__m128i a, __m128i b) {
  return (__m128i)((__keel_i8_128)a > (__keel_i8_128)b);
}
static __inline__ __m128i _mm_set1_epi8(char x) {
  return (__m128i)(__keel_i8_128){x, x, x, x, x, x, x, x,
                                  x, x, x, x, x, x, x, x};
}
typedef unsigned short __keel_u16_128 __attribute__((vector_size(16)));
typedef signed short __keel_i16_128 __attribute__((vector_size(16)));
static __inline__ __m128i _mm_add_epi16(__m128i a, __m128i b) {
  return (__m128i)((__keel_u16_128)a + (__keel_u16_128)b);
}
static __inline__ __m128i _mm_sub_epi16(__m128i a, __m128i b) {
  return (__m128i)((__keel_u16_128)a - (__keel_u16_128)b);
}
static __inline__ __m128i _mm_cmpeq_epi16(__m128i a, __m128i b) {
  return (__m128i)((__keel_i16_128)a == (__keel_i16_128)b);
}
static __inline__ __m128i _mm_cmpgt_epi16(__m128i a, __m128i b) {
  return (__m128i)((__keel_i16_128)a > (__keel_i16_128)b);
}
static __inline__ __m128i _mm_set1_epi16(short x) {
  return (__m128i)(__keel_i16_128){x, x, x, x, x, x, x, x};
}
typedef unsigned int __keel_u32_128 __attribute__((vector_size(16)));
typedef signed int __keel_i32_128 __attribute__((vector_size(16)));
static __inline__ __m128i _mm_add_epi32(__m128i a, __m128i b) {
  return (__m128i)((__keel_u32_128)a + (__keel_u32_128)b);
}
static __inline__ __m128i _mm_sub_epi32(__m128i a, __m128i b) {
  return (__m128i)((__keel_u32_128)a - (__keel_u32_128)b);
}
static __inline__ __m128i _mm_cmpeq_epi32(__m128i a, __m128i b) {
  return (__m128i)((__keel_i32_128)a == (__keel_i32_128)b);
}
static __inline__ __m128i _mm_cmpgt_epi32(__m128i a, __m128i b) {
  return (__m128i)((__keel_i32_128)a > (__keel_i32_128)b);
}
static __inline__ __m128i _mm_set1_epi32(int x) {
  return (__m128i)(__keel_i32_128){x, x, x, x};
}
typedef unsigned long long __keel_u64_128 __attribute__((vector_size(16)));
typedef signed long long __keel_i64_128 __attribute__((vector_size(16)));
static __inline__ __m128i _mm_add_epi64(__m128i a, __m128i b) {
  return (__m128i)((__keel_u64_128)a + (__keel_u64_128)b);
}
static __inline__ __m128i _mm_sub_epi64(__m128i a, __m128i b) {
  return (__m128i)((__keel_u64_128)a - (__keel_u64_128)b);
}
static __inline__ __m128i _mm_cmpeq_epi64(__m128i a, __m128i b) {
  return (__m128i)((__keel_i64_128)a == (__keel_i64_128)b);
}
static __inline__ __m128i _mm_cmpgt_epi64(__m128i a, __m128i b) {
  return (__m128i)((__keel_i64_128)a > (__keel_i64_128)b);
}
static __inline__ __m128i _mm_set1_epi64x(long long x) {
  return (__m128i)(__keel_i64_128){x, x};
}
static __inline__ __m128i _mm_mullo_epi16(__m128i a, __m128i b) {
  return (__m128i)((__keel_u16_128)a * (__keel_u16_128)b);
}
static __inline__ __m128i _mm_mullo_epi32(__m128i a, __m128i b) {
  return (__m128i)((__keel_u32_128)a * (__keel_u32_128)b);
}
static __inline__ __m128i _mm_and_si128(__m128i a, __m128i b) { return a & b; }
static __inline__ __m128i _mm_or_si128(__m128i a, __m128i b) { return a | b; }
static __inline__ __m128i _mm_xor_si128(__m128i a, __m128i b) { return a ^ b; }
static __inline__ __m128i _mm_andnot_si128(__m128i a, __m128i b) {
  return ~a & b;
}
static __inline__ __m128i _mm_setzero_si128(void) { return (__m128i){0}; }
static __inline__ __m128i _mm_load_si128(const __m128i *p) {
  __m128i r;
  __builtin_memcpy(&r, p, 16);
  return r;
}
static __inline__ void _mm_store_si128(__m128i *p, __m128i a) {
  __builtin_memcpy(p, &a, 16);
}
static __inline__ __m128i _mm_loadu_si128(const __m128i *p) {
  __m128i r;
  __builtin_memcpy(&r, p, 16);
  return r;
}
static __inline__ void _mm_storeu_si128(__m128i *p, __m128i a) {
  __builtin_memcpy(p, &a, 16);
}
static __inline__ __m128d _mm_castps_pd(__m128 a) { return (__m128d)a; }
static __inline__ __m128i _mm_castps_si128(__m128 a) { return (__m128i)a; }
static __inline__ __m128 _mm_castpd_ps(__m128d a) { return (__m128)a; }
static __inline__ __m128i _mm_castpd_si128(__m128d a) { return (__m128i)a; }
static __inline__ __m128 _mm_castsi128_ps(__m128i a) { return (__m128)a; }
static __inline__ __m128d _mm_castsi128_pd(__m128i a) { return (__m128d)a; }
#define _mm_shuffle_epi32(a, i)                                                \
  __extension__({                                                              \
    __m128i __keel_a = (a);                                                    \
    (__m128i) __builtin_shufflevector(                                         \
        (__keel_i32_128)__keel_a, (__keel_i32_128)__keel_a, (i) & 3,           \
        ((i) >> 2) & 3, ((i) >> 4) & 3, ((i) >> 6) & 3);                       \
  })
#define _mm_shuffle_pd(a, b, i)                                                \
  __builtin_shufflevector((a), (b), (i) & 1, 2 + (((i) >> 1) & 1))

static __inline__ __m128i _mm_set_epi32(int x3, int x2, int x1, int x0) {
  return (__m128i)(__keel_i32_128){x0, x1, x2, x3};
}
static __inline__ __m128i _mm_set_epi64x(long long x1, long long x0) {
  return (__m128i){x0, x1};
}
static __inline__ int _mm_extract_epi16(__m128i a, int i) {
  return ((__keel_u16_128)a)[i & 7];
}
static __inline__ __m128i _mm_insert_epi16(__m128i a, int x, int i) {
  __keel_u16_128 b = (__keel_u16_128)a;
  b[i & 7] = (unsigned short)x;
  return (__m128i)b;
}
static __inline__ __m128d _mm_and_pd(__m128d a, __m128d b) {
  return (__m128d)((__m128i)a & (__m128i)b);
}
static __inline__ __m128d _mm_or_pd(__m128d a, __m128d b) {
  return (__m128d)((__m128i)a | (__m128i)b);
}
static __inline__ __m128d _mm_xor_pd(__m128d a, __m128d b) {
  return (__m128d)((__m128i)a ^ (__m128i)b);
}
static __inline__ __m128d _mm_andnot_pd(__m128d a, __m128d b) {
  return (__m128d)(~(__m128i)a & (__m128i)b);
}

/* Architectural counts: oversized logical shifts zero; arithmetic shifts sign-fill. */
#define _mm_slli_epi16(a, n) \
  __extension__({ \
    __m128i __keel_shift_a = (a); \
    (__m128i)__builtin_keel_sse2_imm_psllw((__keel_i16_128)__keel_shift_a, (n)); \
  })
static __inline__ __m128i _mm_sll_epi16(__m128i a, __m128i count) {
  return (__m128i)__builtin_keel_sse2_count_psllw((__keel_i16_128)a, ((__keel_u64_128)count)[0]);
}
#define _mm_slli_epi32(a, n) \
  __extension__({ \
    __m128i __keel_shift_a = (a); \
    (__m128i)__builtin_keel_sse2_imm_pslld((__keel_i32_128)__keel_shift_a, (n)); \
  })
static __inline__ __m128i _mm_sll_epi32(__m128i a, __m128i count) {
  return (__m128i)__builtin_keel_sse2_count_pslld((__keel_i32_128)a, ((__keel_u64_128)count)[0]);
}
#define _mm_slli_epi64(a, n) \
  __extension__({ \
    __m128i __keel_shift_a = (a); \
    (__m128i)__builtin_keel_sse2_imm_psllq((__keel_i64_128)__keel_shift_a, (n)); \
  })
static __inline__ __m128i _mm_sll_epi64(__m128i a, __m128i count) {
  return (__m128i)__builtin_keel_sse2_count_psllq((__keel_i64_128)a, ((__keel_u64_128)count)[0]);
}
#define _mm_srli_epi16(a, n) \
  __extension__({ \
    __m128i __keel_shift_a = (a); \
    (__m128i)__builtin_keel_sse2_imm_psrlw((__keel_u16_128)__keel_shift_a, (n)); \
  })
static __inline__ __m128i _mm_srl_epi16(__m128i a, __m128i count) {
  return (__m128i)__builtin_keel_sse2_count_psrlw((__keel_u16_128)a, ((__keel_u64_128)count)[0]);
}
#define _mm_srli_epi32(a, n) \
  __extension__({ \
    __m128i __keel_shift_a = (a); \
    (__m128i)__builtin_keel_sse2_imm_psrld((__keel_u32_128)__keel_shift_a, (n)); \
  })
static __inline__ __m128i _mm_srl_epi32(__m128i a, __m128i count) {
  return (__m128i)__builtin_keel_sse2_count_psrld((__keel_u32_128)a, ((__keel_u64_128)count)[0]);
}
#define _mm_srli_epi64(a, n) \
  __extension__({ \
    __m128i __keel_shift_a = (a); \
    (__m128i)__builtin_keel_sse2_imm_psrlq((__keel_u64_128)__keel_shift_a, (n)); \
  })
static __inline__ __m128i _mm_srl_epi64(__m128i a, __m128i count) {
  return (__m128i)__builtin_keel_sse2_count_psrlq((__keel_u64_128)a, ((__keel_u64_128)count)[0]);
}
#define _mm_srai_epi16(a, n) \
  __extension__({ \
    __m128i __keel_shift_a = (a); \
    (__m128i)__builtin_keel_sse2_imm_psraw((__keel_i16_128)__keel_shift_a, (n)); \
  })
static __inline__ __m128i _mm_sra_epi16(__m128i a, __m128i count) {
  return (__m128i)__builtin_keel_sse2_count_psraw((__keel_i16_128)a, ((__keel_u64_128)count)[0]);
}
#define _mm_srai_epi32(a, n) \
  __extension__({ \
    __m128i __keel_shift_a = (a); \
    (__m128i)__builtin_keel_sse2_imm_psrad((__keel_i32_128)__keel_shift_a, (n)); \
  })
static __inline__ __m128i _mm_sra_epi32(__m128i a, __m128i count) {
  return (__m128i)__builtin_keel_sse2_count_psrad((__keel_i32_128)a, ((__keel_u64_128)count)[0]);
}

/* SSE2 signed/unsigned byte/word saturation uses checked typed operations. */
static __inline__ __m128i _mm_adds_epi8(__m128i a, __m128i b) {
  return (__m128i)__builtin_keel_sse2_paddsb((__keel_i8_128)a, (__keel_i8_128)b);
}
static __inline__ __m128i _mm_adds_epi16(__m128i a, __m128i b) {
  return (__m128i)__builtin_keel_sse2_paddsw((__keel_i16_128)a, (__keel_i16_128)b);
}
static __inline__ __m128i _mm_adds_epu8(__m128i a, __m128i b) {
  return (__m128i)__builtin_keel_sse2_paddusb((__keel_u8_128)a, (__keel_u8_128)b);
}
static __inline__ __m128i _mm_adds_epu16(__m128i a, __m128i b) {
  return (__m128i)__builtin_keel_sse2_paddusw((__keel_u16_128)a, (__keel_u16_128)b);
}
static __inline__ __m128i _mm_subs_epi8(__m128i a, __m128i b) {
  return (__m128i)__builtin_keel_sse2_psubsb((__keel_i8_128)a, (__keel_i8_128)b);
}
static __inline__ __m128i _mm_subs_epi16(__m128i a, __m128i b) {
  return (__m128i)__builtin_keel_sse2_psubsw((__keel_i16_128)a, (__keel_i16_128)b);
}
static __inline__ __m128i _mm_subs_epu8(__m128i a, __m128i b) {
  return (__m128i)__builtin_keel_sse2_psubusb((__keel_u8_128)a, (__keel_u8_128)b);
}
static __inline__ __m128i _mm_subs_epu16(__m128i a, __m128i b) {
  return (__m128i)__builtin_keel_sse2_psubusw((__keel_u16_128)a, (__keel_u16_128)b);
}
#include <__keel_sse2_conversions.h>


/* Packed integer selection preserves all lane bits, including signed minima. */
static __inline__ __m128i _mm_min_epi16(__m128i a, __m128i b) {
  __keel_i16_128 x = (__keel_i16_128)a, y = (__keel_i16_128)b;
  __m128i mask = (__m128i)(x < y);
  return (a & mask) | (b & ~mask);
}
static __inline__ __m128i _mm_max_epi16(__m128i a, __m128i b) {
  __keel_i16_128 x = (__keel_i16_128)a, y = (__keel_i16_128)b;
  __m128i mask = (__m128i)(x > y);
  return (a & mask) | (b & ~mask);
}
static __inline__ __m128i _mm_min_epu8(__m128i a, __m128i b) {
  __keel_u8_128 x = (__keel_u8_128)a, y = (__keel_u8_128)b;
  __m128i mask = (__m128i)(x < y);
  return (a & mask) | (b & ~mask);
}
static __inline__ __m128i _mm_max_epu8(__m128i a, __m128i b) {
  __keel_u8_128 x = (__keel_u8_128)a, y = (__keel_u8_128)b;
  __m128i mask = (__m128i)(x > y);
  return (a & mask) | (b & ~mask);
}

static __inline__ __m128i _mm_avg_epu8(__m128i a, __m128i b) {
  return (__m128i)__builtin_keel_sse2_pavgb128((__keel_i8_128)a, (__keel_i8_128)b);
}
static __inline__ __m128i _mm_avg_epu16(__m128i a, __m128i b) {
  return (__m128i)__builtin_keel_sse2_pavgw128((__keel_i16_128)a, (__keel_i16_128)b);
}
static __inline__ __m128i _mm_madd_epi16(__m128i a, __m128i b) {
  return (__m128i)__builtin_keel_sse2_pmaddwd128((__keel_i16_128)a, (__keel_i16_128)b);
}
static __inline__ __m128i _mm_mulhi_epi16(__m128i a, __m128i b) {
  return (__m128i)__builtin_keel_sse2_pmulhw128((__keel_i16_128)a, (__keel_i16_128)b);
}
static __inline__ __m128i _mm_mulhi_epu16(__m128i a, __m128i b) {
  return (__m128i)__builtin_keel_sse2_pmulhuw128((__keel_i16_128)a, (__keel_i16_128)b);
}
static __inline__ __m128i _mm_packs_epi16(__m128i a, __m128i b) {
  return (__m128i)__builtin_keel_sse2_packsswb128((__keel_i16_128)a, (__keel_i16_128)b);
}
static __inline__ __m128i _mm_packs_epi32(__m128i a, __m128i b) {
  return (__m128i)__builtin_keel_sse2_packssdw128((__keel_i32_128)a, (__keel_i32_128)b);
}
static __inline__ __m128i _mm_packus_epi16(__m128i a, __m128i b) {
  return (__m128i)__builtin_keel_sse2_packuswb128((__keel_i16_128)a, (__keel_i16_128)b);
}
static __inline__ int _mm_movemask_epi8(__m128i a) {
  return __builtin_keel_sse_pmovmskb128((__keel_i8_128)a);
}

static __inline__ __m128i _mm_unpacklo_epi8(__m128i a, __m128i b) {
  return (__m128i)__builtin_shufflevector((__keel_i8_128)a, (__keel_i8_128)b, 0, 16, 1, 17, 2, 18, 3, 19, 4, 20, 5, 21, 6, 22, 7, 23);
}
static __inline__ __m128i _mm_unpackhi_epi8(__m128i a, __m128i b) {
  return (__m128i)__builtin_shufflevector((__keel_i8_128)a, (__keel_i8_128)b, 8, 24, 9, 25, 10, 26, 11, 27, 12, 28, 13, 29, 14, 30, 15, 31);
}
static __inline__ __m128i _mm_unpacklo_epi16(__m128i a, __m128i b) {
  return (__m128i)__builtin_shufflevector((__keel_i16_128)a, (__keel_i16_128)b, 0, 8, 1, 9, 2, 10, 3, 11);
}
static __inline__ __m128i _mm_unpackhi_epi16(__m128i a, __m128i b) {
  return (__m128i)__builtin_shufflevector((__keel_i16_128)a, (__keel_i16_128)b, 4, 12, 5, 13, 6, 14, 7, 15);
}
static __inline__ __m128i _mm_unpacklo_epi32(__m128i a, __m128i b) {
  return (__m128i)__builtin_shufflevector((__keel_i32_128)a, (__keel_i32_128)b, 0, 4, 1, 5);
}
static __inline__ __m128i _mm_unpackhi_epi32(__m128i a, __m128i b) {
  return (__m128i)__builtin_shufflevector((__keel_i32_128)a, (__keel_i32_128)b, 2, 6, 3, 7);
}
static __inline__ __m128i _mm_unpacklo_epi64(__m128i a, __m128i b) {
  return (__m128i)__builtin_shufflevector((__keel_i64_128)a, (__keel_i64_128)b, 0, 2);
}
static __inline__ __m128i _mm_unpackhi_epi64(__m128i a, __m128i b) {
  return (__m128i)__builtin_shufflevector((__keel_i64_128)a, (__keel_i64_128)b, 1, 3);
}
static __inline__ __m128d _mm_unpacklo_pd(__m128d a, __m128d b) {
  return (__m128d)__builtin_shufflevector(a, b, 0, 2);
}
static __inline__ __m128d _mm_unpackhi_pd(__m128d a, __m128d b) {
  return (__m128d)__builtin_shufflevector(a, b, 1, 3);
}

static __inline__ __m128i _mm_mul_epu32(__m128i a, __m128i b) {
  return (__m128i)__builtin_keel_sse2_pmuludq128((__keel_i32_128)a, (__keel_i32_128)b);
}
static __inline__ __m128i _mm_sad_epu8(__m128i a, __m128i b) {
  return (__m128i)__builtin_keel_sse2_psadbw128((__keel_i8_128)a, (__keel_i8_128)b);
}
static __inline__ __m128d _mm_sqrt_pd(__m128d a) {
  return __builtin_keel_sse_sqrtpd128(a);
}

#endif /* _KEEL_EMMINTRIN_H */
