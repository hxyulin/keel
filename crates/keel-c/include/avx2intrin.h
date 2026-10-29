/* Bounded Keel intrinsic subset; see docs/c-x86-intrinsics.md. */
#ifndef _KEEL_AVX2INTRIN_H
#define _KEEL_AVX2INTRIN_H
#ifndef __x86_64__
#error "Keel x86 intrinsic headers require x86-64"
#endif
#include <avxintrin.h>
typedef unsigned char __keel_u8_256 __attribute__((vector_size(32)));
typedef signed char __keel_i8_256 __attribute__((vector_size(32)));
static __inline__ __m256i _mm256_add_epi8(__m256i a, __m256i b) {
  return (__m256i)((__keel_u8_256)a + (__keel_u8_256)b);
}
static __inline__ __m256i _mm256_sub_epi8(__m256i a, __m256i b) {
  return (__m256i)((__keel_u8_256)a - (__keel_u8_256)b);
}
static __inline__ __m256i _mm256_cmpeq_epi8(__m256i a, __m256i b) {
  return (__m256i)((__keel_i8_256)a == (__keel_i8_256)b);
}
static __inline__ __m256i _mm256_cmpgt_epi8(__m256i a, __m256i b) {
  return (__m256i)((__keel_i8_256)a > (__keel_i8_256)b);
}
static __inline__ __m256i _mm256_set1_epi8(char x) {
  return (__m256i)(__keel_i8_256){x, x, x, x, x, x, x, x, x, x, x,
                                  x, x, x, x, x, x, x, x, x, x, x,
                                  x, x, x, x, x, x, x, x, x, x};
}
typedef unsigned short __keel_u16_256 __attribute__((vector_size(32)));
typedef signed short __keel_i16_256 __attribute__((vector_size(32)));
static __inline__ __m256i _mm256_add_epi16(__m256i a, __m256i b) {
  return (__m256i)((__keel_u16_256)a + (__keel_u16_256)b);
}
static __inline__ __m256i _mm256_sub_epi16(__m256i a, __m256i b) {
  return (__m256i)((__keel_u16_256)a - (__keel_u16_256)b);
}
static __inline__ __m256i _mm256_cmpeq_epi16(__m256i a, __m256i b) {
  return (__m256i)((__keel_i16_256)a == (__keel_i16_256)b);
}
static __inline__ __m256i _mm256_cmpgt_epi16(__m256i a, __m256i b) {
  return (__m256i)((__keel_i16_256)a > (__keel_i16_256)b);
}
static __inline__ __m256i _mm256_set1_epi16(short x) {
  return (__m256i)(__keel_i16_256){x, x, x, x, x, x, x, x,
                                   x, x, x, x, x, x, x, x};
}
typedef unsigned int __keel_u32_256 __attribute__((vector_size(32)));
typedef signed int __keel_i32_256 __attribute__((vector_size(32)));
static __inline__ __m256i _mm256_add_epi32(__m256i a, __m256i b) {
  return (__m256i)((__keel_u32_256)a + (__keel_u32_256)b);
}
static __inline__ __m256i _mm256_sub_epi32(__m256i a, __m256i b) {
  return (__m256i)((__keel_u32_256)a - (__keel_u32_256)b);
}
static __inline__ __m256i _mm256_cmpeq_epi32(__m256i a, __m256i b) {
  return (__m256i)((__keel_i32_256)a == (__keel_i32_256)b);
}
static __inline__ __m256i _mm256_cmpgt_epi32(__m256i a, __m256i b) {
  return (__m256i)((__keel_i32_256)a > (__keel_i32_256)b);
}
static __inline__ __m256i _mm256_set1_epi32(int x) {
  return (__m256i)(__keel_i32_256){x, x, x, x, x, x, x, x};
}
typedef unsigned long long __keel_u64_256 __attribute__((vector_size(32)));
typedef signed long long __keel_i64_256 __attribute__((vector_size(32)));
static __inline__ __m256i _mm256_add_epi64(__m256i a, __m256i b) {
  return (__m256i)((__keel_u64_256)a + (__keel_u64_256)b);
}
static __inline__ __m256i _mm256_sub_epi64(__m256i a, __m256i b) {
  return (__m256i)((__keel_u64_256)a - (__keel_u64_256)b);
}
static __inline__ __m256i _mm256_cmpeq_epi64(__m256i a, __m256i b) {
  return (__m256i)((__keel_i64_256)a == (__keel_i64_256)b);
}
static __inline__ __m256i _mm256_cmpgt_epi64(__m256i a, __m256i b) {
  return (__m256i)((__keel_i64_256)a > (__keel_i64_256)b);
}
static __inline__ __m256i _mm256_set1_epi64x(long long x) {
  return (__m256i)(__keel_i64_256){x, x, x, x};
}
static __inline__ __m256i _mm256_mullo_epi16(__m256i a, __m256i b) {
  return (__m256i)((__keel_u16_256)a * (__keel_u16_256)b);
}
static __inline__ __m256i _mm256_mullo_epi32(__m256i a, __m256i b) {
  return (__m256i)((__keel_u32_256)a * (__keel_u32_256)b);
}
static __inline__ __m256i _mm256_and_si256(__m256i a, __m256i b) {
  return a & b;
}
static __inline__ __m256i _mm256_or_si256(__m256i a, __m256i b) {
  return a | b;
}
static __inline__ __m256i _mm256_xor_si256(__m256i a, __m256i b) {
  return a ^ b;
}
static __inline__ __m256i _mm256_andnot_si256(__m256i a, __m256i b) {
  return ~a & b;
}
static __inline__ __m256i _mm256_setzero_si256(void) { return (__m256i){0}; }
static __inline__ __m256i _mm256_load_si256(const __m256i *p) {
  __m256i r;
  __builtin_memcpy(&r, p, 32);
  return r;
}
static __inline__ void _mm256_store_si256(__m256i *p, __m256i a) {
  __builtin_memcpy(p, &a, 32);
}
static __inline__ __m256i _mm256_loadu_si256(const __m256i *p) {
  __m256i r;
  __builtin_memcpy(&r, p, 32);
  return r;
}
static __inline__ void _mm256_storeu_si256(__m256i *p, __m256i a) {
  __builtin_memcpy(p, &a, 32);
}
#define _mm256_shuffle_epi32(a, i)                                             \
  __extension__({                                                              \
    __m256i __keel_a = (a);                                                    \
    (__m256i) __builtin_shufflevector(                                         \
        (__keel_i32_256)__keel_a, (__keel_i32_256)__keel_a, (i) & 3,           \
        ((i) >> 2) & 3, ((i) >> 4) & 3, ((i) >> 6) & 3, 4 + ((i) & 3),         \
        4 + (((i) >> 2) & 3), 4 + (((i) >> 4) & 3), 4 + (((i) >> 6) & 3));     \
  })

static __inline__ __m256i _mm256_set_epi32(int x7, int x6, int x5, int x4,
                                           int x3, int x2, int x1, int x0) {
  return (__m256i)(__keel_i32_256){x0, x1, x2, x3, x4, x5, x6, x7};
}
static __inline__ __m256i _mm256_set_epi64x(long long x3, long long x2,
                                            long long x1, long long x0) {
  return (__m256i){x0, x1, x2, x3};
}
static __inline__ __m128i _mm256_extracti128_si256(__m256i a, int i) {
  return _mm256_extractf128_si256(a, i);
}
static __inline__ __m256i _mm256_inserti128_si256(__m256i a, __m128i b, int i) {
  return _mm256_insertf128_si256(a, b, i);
}
#define _mm256_permute4x64_epi64(a, i)                                         \
  __extension__({                                                              \
    __m256i __keel_a = (a);                                                    \
    __builtin_shufflevector(__keel_a, __keel_a, (i) & 3, ((i) >> 2) & 3,       \
                            ((i) >> 4) & 3, ((i) >> 6) & 3);                   \
  })

static __inline__ __m256i _mm256_min_epi8(__m256i a, __m256i b) {
  __keel_i8_256 x = (__keel_i8_256)a, y = (__keel_i8_256)b;
  __m256i mask = (__m256i)(x < y);
  return (a & mask) | (b & ~mask);
}
static __inline__ __m256i _mm256_max_epi8(__m256i a, __m256i b) {
  __keel_i8_256 x = (__keel_i8_256)a, y = (__keel_i8_256)b;
  __m256i mask = (__m256i)(x > y);
  return (a & mask) | (b & ~mask);
}
static __inline__ __m256i _mm256_min_epu8(__m256i a, __m256i b) {
  __keel_u8_256 x = (__keel_u8_256)a, y = (__keel_u8_256)b;
  __m256i mask = (__m256i)(x < y);
  return (a & mask) | (b & ~mask);
}
static __inline__ __m256i _mm256_max_epu8(__m256i a, __m256i b) {
  __keel_u8_256 x = (__keel_u8_256)a, y = (__keel_u8_256)b;
  __m256i mask = (__m256i)(x > y);
  return (a & mask) | (b & ~mask);
}
static __inline__ __m256i _mm256_min_epi16(__m256i a, __m256i b) {
  __keel_i16_256 x = (__keel_i16_256)a, y = (__keel_i16_256)b;
  __m256i mask = (__m256i)(x < y);
  return (a & mask) | (b & ~mask);
}
static __inline__ __m256i _mm256_max_epi16(__m256i a, __m256i b) {
  __keel_i16_256 x = (__keel_i16_256)a, y = (__keel_i16_256)b;
  __m256i mask = (__m256i)(x > y);
  return (a & mask) | (b & ~mask);
}
static __inline__ __m256i _mm256_min_epu16(__m256i a, __m256i b) {
  __keel_u16_256 x = (__keel_u16_256)a, y = (__keel_u16_256)b;
  __m256i mask = (__m256i)(x < y);
  return (a & mask) | (b & ~mask);
}
static __inline__ __m256i _mm256_max_epu16(__m256i a, __m256i b) {
  __keel_u16_256 x = (__keel_u16_256)a, y = (__keel_u16_256)b;
  __m256i mask = (__m256i)(x > y);
  return (a & mask) | (b & ~mask);
}
static __inline__ __m256i _mm256_min_epi32(__m256i a, __m256i b) {
  __keel_i32_256 x = (__keel_i32_256)a, y = (__keel_i32_256)b;
  __m256i mask = (__m256i)(x < y);
  return (a & mask) | (b & ~mask);
}
static __inline__ __m256i _mm256_max_epi32(__m256i a, __m256i b) {
  __keel_i32_256 x = (__keel_i32_256)a, y = (__keel_i32_256)b;
  __m256i mask = (__m256i)(x > y);
  return (a & mask) | (b & ~mask);
}
static __inline__ __m256i _mm256_min_epu32(__m256i a, __m256i b) {
  __keel_u32_256 x = (__keel_u32_256)a, y = (__keel_u32_256)b;
  __m256i mask = (__m256i)(x < y);
  return (a & mask) | (b & ~mask);
}
static __inline__ __m256i _mm256_max_epu32(__m256i a, __m256i b) {
  __keel_u32_256 x = (__keel_u32_256)a, y = (__keel_u32_256)b;
  __m256i mask = (__m256i)(x > y);
  return (a & mask) | (b & ~mask);
}
static __inline__ __m256i _mm256_abs_epi8(__m256i a) {
  __keel_u8_256 x = (__keel_u8_256)a;
  __keel_u8_256 negative = (__keel_u8_256)((__keel_i8_256)a < (__keel_i8_256){0});
  return (__m256i)((x & ~negative) | (((__keel_u8_256){0} - x) & negative));
}
static __inline__ __m256i _mm256_sign_epi8(__m256i a, __m256i b) {
  __keel_u8_256 x = (__keel_u8_256)a;
  __keel_i8_256 selector = (__keel_i8_256)b;
  __keel_u8_256 positive = (__keel_u8_256)(selector > (__keel_i8_256){0});
  __keel_u8_256 negative = (__keel_u8_256)(selector < (__keel_i8_256){0});
  return (__m256i)((x & positive) | (((__keel_u8_256){0} - x) & negative));
}
static __inline__ __m256i _mm256_abs_epi16(__m256i a) {
  __keel_u16_256 x = (__keel_u16_256)a;
  __keel_u16_256 negative = (__keel_u16_256)((__keel_i16_256)a < (__keel_i16_256){0});
  return (__m256i)((x & ~negative) | (((__keel_u16_256){0} - x) & negative));
}
static __inline__ __m256i _mm256_sign_epi16(__m256i a, __m256i b) {
  __keel_u16_256 x = (__keel_u16_256)a;
  __keel_i16_256 selector = (__keel_i16_256)b;
  __keel_u16_256 positive = (__keel_u16_256)(selector > (__keel_i16_256){0});
  __keel_u16_256 negative = (__keel_u16_256)(selector < (__keel_i16_256){0});
  return (__m256i)((x & positive) | (((__keel_u16_256){0} - x) & negative));
}
static __inline__ __m256i _mm256_abs_epi32(__m256i a) {
  __keel_u32_256 x = (__keel_u32_256)a;
  __keel_u32_256 negative = (__keel_u32_256)((__keel_i32_256)a < (__keel_i32_256){0});
  return (__m256i)((x & ~negative) | (((__keel_u32_256){0} - x) & negative));
}
static __inline__ __m256i _mm256_sign_epi32(__m256i a, __m256i b) {
  __keel_u32_256 x = (__keel_u32_256)a;
  __keel_i32_256 selector = (__keel_i32_256)b;
  __keel_u32_256 positive = (__keel_u32_256)(selector > (__keel_i32_256){0});
  __keel_u32_256 negative = (__keel_u32_256)(selector < (__keel_i32_256){0});
  return (__m256i)((x & positive) | (((__keel_u32_256){0} - x) & negative));
}
static __inline__ __m256i _mm256_blendv_epi8(__m256i a, __m256i b, __m256i selector) {
  __m256i mask = (__m256i)((__keel_i8_256)selector < (__keel_i8_256){0});
  return (a & ~mask) | (b & mask);
}

/* Packed counts use the low unsigned 64 bits; per-lane counts use all bits. */
#define _mm256_slli_epi16(a, n) \
  __extension__({ \
    __m256i __keel_shift_a = (a); \
    (__m256i)__builtin_keel_avx2_imm_psllw((__keel_i16_256)__keel_shift_a, (n)); \
  })
static __inline__ __m256i _mm256_sll_epi16(__m256i a, __m128i count) {
  return (__m256i)__builtin_keel_avx2_count_psllw((__keel_i16_256)a, ((__keel_u64_128)count)[0]);
}
#define _mm256_slli_epi32(a, n) \
  __extension__({ \
    __m256i __keel_shift_a = (a); \
    (__m256i)__builtin_keel_avx2_imm_pslld((__keel_i32_256)__keel_shift_a, (n)); \
  })
static __inline__ __m256i _mm256_sll_epi32(__m256i a, __m128i count) {
  return (__m256i)__builtin_keel_avx2_count_pslld((__keel_i32_256)a, ((__keel_u64_128)count)[0]);
}
#define _mm256_slli_epi64(a, n) \
  __extension__({ \
    __m256i __keel_shift_a = (a); \
    (__m256i)__builtin_keel_avx2_imm_psllq((__keel_i64_256)__keel_shift_a, (n)); \
  })
static __inline__ __m256i _mm256_sll_epi64(__m256i a, __m128i count) {
  return (__m256i)__builtin_keel_avx2_count_psllq((__keel_i64_256)a, ((__keel_u64_128)count)[0]);
}
#define _mm256_srli_epi16(a, n) \
  __extension__({ \
    __m256i __keel_shift_a = (a); \
    (__m256i)__builtin_keel_avx2_imm_psrlw((__keel_i16_256)__keel_shift_a, (n)); \
  })
static __inline__ __m256i _mm256_srl_epi16(__m256i a, __m128i count) {
  return (__m256i)__builtin_keel_avx2_count_psrlw((__keel_i16_256)a, ((__keel_u64_128)count)[0]);
}
#define _mm256_srli_epi32(a, n) \
  __extension__({ \
    __m256i __keel_shift_a = (a); \
    (__m256i)__builtin_keel_avx2_imm_psrld((__keel_i32_256)__keel_shift_a, (n)); \
  })
static __inline__ __m256i _mm256_srl_epi32(__m256i a, __m128i count) {
  return (__m256i)__builtin_keel_avx2_count_psrld((__keel_i32_256)a, ((__keel_u64_128)count)[0]);
}
#define _mm256_srli_epi64(a, n) \
  __extension__({ \
    __m256i __keel_shift_a = (a); \
    (__m256i)__builtin_keel_avx2_imm_psrlq((__keel_i64_256)__keel_shift_a, (n)); \
  })
static __inline__ __m256i _mm256_srl_epi64(__m256i a, __m128i count) {
  return (__m256i)__builtin_keel_avx2_count_psrlq((__keel_i64_256)a, ((__keel_u64_128)count)[0]);
}
#define _mm256_srai_epi16(a, n) \
  __extension__({ \
    __m256i __keel_shift_a = (a); \
    (__m256i)__builtin_keel_avx2_imm_psraw((__keel_i16_256)__keel_shift_a, (n)); \
  })
static __inline__ __m256i _mm256_sra_epi16(__m256i a, __m128i count) {
  return (__m256i)__builtin_keel_avx2_count_psraw((__keel_i16_256)a, ((__keel_u64_128)count)[0]);
}
#define _mm256_srai_epi32(a, n) \
  __extension__({ \
    __m256i __keel_shift_a = (a); \
    (__m256i)__builtin_keel_avx2_imm_psrad((__keel_i32_256)__keel_shift_a, (n)); \
  })
static __inline__ __m256i _mm256_sra_epi32(__m256i a, __m128i count) {
  return (__m256i)__builtin_keel_avx2_count_psrad((__keel_i32_256)a, ((__keel_u64_128)count)[0]);
}
static __inline__ __m128i _mm_sllv_epi32(__m128i a, __m128i count) {
  return (__m128i)__builtin_keel_avx2_psllvd128((__keel_i32_128)a, (__keel_i32_128)count);
}
static __inline__ __m128i _mm_sllv_epi64(__m128i a, __m128i count) {
  return (__m128i)__builtin_keel_avx2_psllvq128((__keel_i64_128)a, (__keel_i64_128)count);
}
static __inline__ __m128i _mm_srlv_epi32(__m128i a, __m128i count) {
  return (__m128i)__builtin_keel_avx2_psrlvd128((__keel_i32_128)a, (__keel_i32_128)count);
}
static __inline__ __m128i _mm_srlv_epi64(__m128i a, __m128i count) {
  return (__m128i)__builtin_keel_avx2_psrlvq128((__keel_i64_128)a, (__keel_i64_128)count);
}
static __inline__ __m128i _mm_srav_epi32(__m128i a, __m128i count) {
  return (__m128i)__builtin_keel_avx2_psravd128((__keel_i32_128)a, (__keel_i32_128)count);
}
static __inline__ __m256i _mm256_sllv_epi32(__m256i a, __m256i count) {
  return (__m256i)__builtin_keel_avx2_psllvd256((__keel_i32_256)a, (__keel_i32_256)count);
}
static __inline__ __m256i _mm256_sllv_epi64(__m256i a, __m256i count) {
  return (__m256i)__builtin_keel_avx2_psllvq256((__keel_i64_256)a, (__keel_i64_256)count);
}
static __inline__ __m256i _mm256_srlv_epi32(__m256i a, __m256i count) {
  return (__m256i)__builtin_keel_avx2_psrlvd256((__keel_i32_256)a, (__keel_i32_256)count);
}
static __inline__ __m256i _mm256_srlv_epi64(__m256i a, __m256i count) {
  return (__m256i)__builtin_keel_avx2_psrlvq256((__keel_i64_256)a, (__keel_i64_256)count);
}
static __inline__ __m256i _mm256_srav_epi32(__m256i a, __m256i count) {
  return (__m256i)__builtin_keel_avx2_psravd256((__keel_i32_256)a, (__keel_i32_256)count);
}

static __inline__ __m256i _mm256_adds_epi8(__m256i a, __m256i b) {
  return (__m256i)__builtin_keel_avx2_vpaddsb256((__keel_i8_256)a, (__keel_i8_256)b);
}
static __inline__ __m256i _mm256_adds_epu8(__m256i a, __m256i b) {
  return (__m256i)__builtin_keel_avx2_vpaddusb256((__keel_i8_256)a, (__keel_i8_256)b);
}
static __inline__ __m256i _mm256_subs_epi8(__m256i a, __m256i b) {
  return (__m256i)__builtin_keel_avx2_vpsubsb256((__keel_i8_256)a, (__keel_i8_256)b);
}
static __inline__ __m256i _mm256_subs_epu8(__m256i a, __m256i b) {
  return (__m256i)__builtin_keel_avx2_vpsubusb256((__keel_i8_256)a, (__keel_i8_256)b);
}
static __inline__ __m256i _mm256_avg_epu8(__m256i a, __m256i b) {
  return (__m256i)__builtin_keel_avx2_vpavgb256((__keel_i8_256)a, (__keel_i8_256)b);
}
static __inline__ __m256i _mm256_adds_epi16(__m256i a, __m256i b) {
  return (__m256i)__builtin_keel_avx2_vpaddsw256((__keel_i16_256)a, (__keel_i16_256)b);
}
static __inline__ __m256i _mm256_adds_epu16(__m256i a, __m256i b) {
  return (__m256i)__builtin_keel_avx2_vpaddusw256((__keel_i16_256)a, (__keel_i16_256)b);
}
static __inline__ __m256i _mm256_subs_epi16(__m256i a, __m256i b) {
  return (__m256i)__builtin_keel_avx2_vpsubsw256((__keel_i16_256)a, (__keel_i16_256)b);
}
static __inline__ __m256i _mm256_subs_epu16(__m256i a, __m256i b) {
  return (__m256i)__builtin_keel_avx2_vpsubusw256((__keel_i16_256)a, (__keel_i16_256)b);
}
static __inline__ __m256i _mm256_avg_epu16(__m256i a, __m256i b) {
  return (__m256i)__builtin_keel_avx2_vpavgw256((__keel_i16_256)a, (__keel_i16_256)b);
}
static __inline__ __m256i _mm256_madd_epi16(__m256i a, __m256i b) {
  return (__m256i)__builtin_keel_avx2_vpmaddwd256((__keel_i16_256)a, (__keel_i16_256)b);
}
static __inline__ __m256i _mm256_maddubs_epi16(__m256i a, __m256i b) {
  return (__m256i)__builtin_keel_avx2_vpmaddubsw256((__keel_i8_256)a, (__keel_i8_256)b);
}
static __inline__ __m256i _mm256_mulhi_epi16(__m256i a, __m256i b) {
  return (__m256i)__builtin_keel_avx2_vpmulhw256((__keel_i16_256)a, (__keel_i16_256)b);
}
static __inline__ __m256i _mm256_mulhi_epu16(__m256i a, __m256i b) {
  return (__m256i)__builtin_keel_avx2_vpmulhuw256((__keel_i16_256)a, (__keel_i16_256)b);
}
static __inline__ __m256i _mm256_mulhrs_epi16(__m256i a, __m256i b) {
  return (__m256i)__builtin_keel_avx2_vpmulhrsw256((__keel_i16_256)a, (__keel_i16_256)b);
}
static __inline__ __m256i _mm256_packs_epi16(__m256i a, __m256i b) {
  return (__m256i)__builtin_keel_avx2_vpacksswb256((__keel_i16_256)a, (__keel_i16_256)b);
}
static __inline__ __m256i _mm256_packs_epi32(__m256i a, __m256i b) {
  return (__m256i)__builtin_keel_avx2_vpackssdw256((__keel_i32_256)a, (__keel_i32_256)b);
}
static __inline__ __m256i _mm256_packus_epi16(__m256i a, __m256i b) {
  return (__m256i)__builtin_keel_avx2_vpackuswb256((__keel_i16_256)a, (__keel_i16_256)b);
}
static __inline__ __m256i _mm256_packus_epi32(__m256i a, __m256i b) {
  return (__m256i)__builtin_keel_avx2_vpackusdw256((__keel_i32_256)a, (__keel_i32_256)b);
}
static __inline__ __m256i _mm256_shuffle_epi8(__m256i a, __m256i b) {
  return (__m256i)__builtin_keel_avx2_vpshufb256((__keel_i8_256)a, (__keel_i8_256)b);
}
static __inline__ int _mm256_movemask_epi8(__m256i a) {
  return __builtin_keel_avx2_vpmovmskb256((__keel_i8_256)a);
}

static __inline__ __m256i _mm256_hadd_epi16(__m256i a, __m256i b) {
  return (__m256i)__builtin_keel_avx2_vphaddw256((__keel_i16_256)a, (__keel_i16_256)b);
}
static __inline__ __m256i _mm256_hadd_epi32(__m256i a, __m256i b) {
  return (__m256i)__builtin_keel_avx2_vphaddd256((__keel_i32_256)a, (__keel_i32_256)b);
}
static __inline__ __m256i _mm256_hadds_epi16(__m256i a, __m256i b) {
  return (__m256i)__builtin_keel_avx2_vphaddsw256((__keel_i16_256)a, (__keel_i16_256)b);
}
static __inline__ __m256i _mm256_hsub_epi16(__m256i a, __m256i b) {
  return (__m256i)__builtin_keel_avx2_vphsubw256((__keel_i16_256)a, (__keel_i16_256)b);
}
static __inline__ __m256i _mm256_hsub_epi32(__m256i a, __m256i b) {
  return (__m256i)__builtin_keel_avx2_vphsubd256((__keel_i32_256)a, (__keel_i32_256)b);
}
static __inline__ __m256i _mm256_hsubs_epi16(__m256i a, __m256i b) {
  return (__m256i)__builtin_keel_avx2_vphsubsw256((__keel_i16_256)a, (__keel_i16_256)b);
}
static __inline__ __m256i _mm256_unpacklo_epi8(__m256i a, __m256i b) {
  return (__m256i)__builtin_shufflevector((__keel_i8_256)a, (__keel_i8_256)b, 0, 32, 1, 33, 2, 34, 3, 35, 4, 36, 5, 37, 6, 38, 7, 39, 16, 48, 17, 49, 18, 50, 19, 51, 20, 52, 21, 53, 22, 54, 23, 55);
}
static __inline__ __m256i _mm256_unpackhi_epi8(__m256i a, __m256i b) {
  return (__m256i)__builtin_shufflevector((__keel_i8_256)a, (__keel_i8_256)b, 8, 40, 9, 41, 10, 42, 11, 43, 12, 44, 13, 45, 14, 46, 15, 47, 24, 56, 25, 57, 26, 58, 27, 59, 28, 60, 29, 61, 30, 62, 31, 63);
}
static __inline__ __m256i _mm256_unpacklo_epi16(__m256i a, __m256i b) {
  return (__m256i)__builtin_shufflevector((__keel_i16_256)a, (__keel_i16_256)b, 0, 16, 1, 17, 2, 18, 3, 19, 8, 24, 9, 25, 10, 26, 11, 27);
}
static __inline__ __m256i _mm256_unpackhi_epi16(__m256i a, __m256i b) {
  return (__m256i)__builtin_shufflevector((__keel_i16_256)a, (__keel_i16_256)b, 4, 20, 5, 21, 6, 22, 7, 23, 12, 28, 13, 29, 14, 30, 15, 31);
}
static __inline__ __m256i _mm256_unpacklo_epi32(__m256i a, __m256i b) {
  return (__m256i)__builtin_shufflevector((__keel_i32_256)a, (__keel_i32_256)b, 0, 8, 1, 9, 4, 12, 5, 13);
}
static __inline__ __m256i _mm256_unpackhi_epi32(__m256i a, __m256i b) {
  return (__m256i)__builtin_shufflevector((__keel_i32_256)a, (__keel_i32_256)b, 2, 10, 3, 11, 6, 14, 7, 15);
}
static __inline__ __m256i _mm256_unpacklo_epi64(__m256i a, __m256i b) {
  return (__m256i)__builtin_shufflevector((__keel_i64_256)a, (__keel_i64_256)b, 0, 4, 2, 6);
}
static __inline__ __m256i _mm256_unpackhi_epi64(__m256i a, __m256i b) {
  return (__m256i)__builtin_shufflevector((__keel_i64_256)a, (__keel_i64_256)b, 1, 5, 3, 7);
}

static __inline__ __m256i _mm256_mul_epi32(__m256i a, __m256i b) {
  return (__m256i)__builtin_keel_avx2_vpmuldq256((__keel_i32_256)a, (__keel_i32_256)b);
}
static __inline__ __m256i _mm256_mul_epu32(__m256i a, __m256i b) {
  return (__m256i)__builtin_keel_avx2_vpmuludq256((__keel_i32_256)a, (__keel_i32_256)b);
}
static __inline__ __m256i _mm256_sad_epu8(__m256i a, __m256i b) {
  return (__m256i)__builtin_keel_avx2_vpsadbw256((__keel_i8_256)a, (__keel_i8_256)b);
}

#endif /* _KEEL_AVX2INTRIN_H */
