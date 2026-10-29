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

#endif /* _KEEL_AVX2INTRIN_H */
