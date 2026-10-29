/* Inactive lanes suppress memory accesses; architectural builtins own effects.
 */
#ifndef __KEEL_AVX2_MASK_MEMORY_H
#define __KEEL_AVX2_MASK_MEMORY_H

static __inline__ __attribute__((always_inline)) __m128i _mm_maskload_epi32(const int *p, __m128i mask) {
  return (__m128i)__builtin_keel_mask_avx2vpmaskmovdload128(
      (const void *)p, (__keel_i32_128)mask);
}

static __inline__ __attribute__((always_inline)) void _mm_maskstore_epi32(int *p, __m128i mask, __m128i data) {
  __builtin_keel_mask_avx2vpmaskmovdstore128((void *)p, (__keel_i32_128)data,
                                             (__keel_i32_128)mask);
}

static __inline__ __attribute__((always_inline)) __m256i _mm256_maskload_epi32(const int *p, __m256i mask) {
  return (__m256i)__builtin_keel_mask_avx2vpmaskmovdload256(
      (const void *)p, (__keel_i32_256)mask);
}

static __inline__ __attribute__((always_inline)) void _mm256_maskstore_epi32(int *p, __m256i mask,
                                              __m256i data) {
  __builtin_keel_mask_avx2vpmaskmovdstore256((void *)p, (__keel_i32_256)data,
                                             (__keel_i32_256)mask);
}

static __inline__ __attribute__((always_inline)) __m128i _mm_maskload_epi64(const long long *p, __m128i mask) {
  return (__m128i)__builtin_keel_mask_avx2vpmaskmovqload128(
      (const void *)p, (__keel_i64_128)mask);
}

static __inline__ __attribute__((always_inline)) void _mm_maskstore_epi64(long long *p, __m128i mask,
                                           __m128i data) {
  __builtin_keel_mask_avx2vpmaskmovqstore128((void *)p, (__keel_i64_128)data,
                                             (__keel_i64_128)mask);
}

static __inline__ __attribute__((always_inline)) __m256i _mm256_maskload_epi64(const long long *p,
                                                __m256i mask) {
  return (__m256i)__builtin_keel_mask_avx2vpmaskmovqload256(
      (const void *)p, (__keel_i64_256)mask);
}

static __inline__ __attribute__((always_inline)) void _mm256_maskstore_epi64(long long *p, __m256i mask,
                                              __m256i data) {
  __builtin_keel_mask_avx2vpmaskmovqstore256((void *)p, (__keel_i64_256)data,
                                             (__keel_i64_256)mask);
}

#define _mm_i32gather_ps(base, index, scale)                                   \
  ((__m128)__builtin_keel_gather_avx2vgatherdps128(                            \
      (const void *)(base), (__keel_i32_128)(index), (__m128)((__m128){0}),    \
      (__keel_i32_128)(~(__m128i){0}), (scale)))

#define _mm_mask_i32gather_ps(src, base, index, mask, scale)                   \
  ((__m128)__builtin_keel_gather_avx2vgatherdps128(                            \
      (const void *)(base), (__keel_i32_128)(index), (__m128)(src),            \
      (__keel_i32_128)(mask), (scale)))

#define _mm256_i32gather_ps(base, index, scale)                                \
  ((__m256)__builtin_keel_gather_avx2vgatherdps256(                            \
      (const void *)(base), (__keel_i32_256)(index), (__m256)((__m256){0}),    \
      (__keel_i32_256)(~(__m256i){0}), (scale)))

#define _mm256_mask_i32gather_ps(src, base, index, mask, scale)                \
  ((__m256)__builtin_keel_gather_avx2vgatherdps256(                            \
      (const void *)(base), (__keel_i32_256)(index), (__m256)(src),            \
      (__keel_i32_256)(mask), (scale)))

#define _mm_i32gather_pd(base, index, scale)                                   \
  ((__m128d)__builtin_keel_gather_avx2vgatherdpd128(                           \
      (const void *)(base), (__keel_i32_128)(index), (__m128d)((__m128d){0}),  \
      (__keel_i64_128)(~(__m128i){0}), (scale)))

#define _mm_mask_i32gather_pd(src, base, index, mask, scale)                   \
  ((__m128d)__builtin_keel_gather_avx2vgatherdpd128(                           \
      (const void *)(base), (__keel_i32_128)(index), (__m128d)(src),           \
      (__keel_i64_128)(mask), (scale)))

#define _mm256_i32gather_pd(base, index, scale)                                \
  ((__m256d)__builtin_keel_gather_avx2vgatherdpd256(                           \
      (const void *)(base), (__keel_i32_128)(index), (__m256d)((__m256d){0}),  \
      (__keel_i64_256)(~(__m256i){0}), (scale)))

#define _mm256_mask_i32gather_pd(src, base, index, mask, scale)                \
  ((__m256d)__builtin_keel_gather_avx2vgatherdpd256(                           \
      (const void *)(base), (__keel_i32_128)(index), (__m256d)(src),           \
      (__keel_i64_256)(mask), (scale)))

#define _mm_i32gather_epi32(base, index, scale)                                \
  ((__m128i)__builtin_keel_gather_avx2vpgatherdd128(                           \
      (const void *)(base), (__keel_i32_128)(index),                           \
      (__keel_i32_128)((__m128i){0}), (__keel_i32_128)(~(__m128i){0}),         \
      (scale)))

#define _mm_mask_i32gather_epi32(src, base, index, mask, scale)                \
  ((__m128i)__builtin_keel_gather_avx2vpgatherdd128(                           \
      (const void *)(base), (__keel_i32_128)(index), (__keel_i32_128)(src),    \
      (__keel_i32_128)(mask), (scale)))

#define _mm256_i32gather_epi32(base, index, scale)                             \
  ((__m256i)__builtin_keel_gather_avx2vpgatherdd256(                           \
      (const void *)(base), (__keel_i32_256)(index),                           \
      (__keel_i32_256)((__m256i){0}), (__keel_i32_256)(~(__m256i){0}),         \
      (scale)))

#define _mm256_mask_i32gather_epi32(src, base, index, mask, scale)             \
  ((__m256i)__builtin_keel_gather_avx2vpgatherdd256(                           \
      (const void *)(base), (__keel_i32_256)(index), (__keel_i32_256)(src),    \
      (__keel_i32_256)(mask), (scale)))

#define _mm_i32gather_epi64(base, index, scale)                                \
  ((__m128i)__builtin_keel_gather_avx2vpgatherdq128(                           \
      (const void *)(base), (__keel_i32_128)(index),                           \
      (__keel_i64_128)((__m128i){0}), (__keel_i64_128)(~(__m128i){0}),         \
      (scale)))

#define _mm_mask_i32gather_epi64(src, base, index, mask, scale)                \
  ((__m128i)__builtin_keel_gather_avx2vpgatherdq128(                           \
      (const void *)(base), (__keel_i32_128)(index), (__keel_i64_128)(src),    \
      (__keel_i64_128)(mask), (scale)))

#define _mm256_i32gather_epi64(base, index, scale)                             \
  ((__m256i)__builtin_keel_gather_avx2vpgatherdq256(                           \
      (const void *)(base), (__keel_i32_128)(index),                           \
      (__keel_i64_256)((__m256i){0}), (__keel_i64_256)(~(__m256i){0}),         \
      (scale)))

#define _mm256_mask_i32gather_epi64(src, base, index, mask, scale)             \
  ((__m256i)__builtin_keel_gather_avx2vpgatherdq256(                           \
      (const void *)(base), (__keel_i32_128)(index), (__keel_i64_256)(src),    \
      (__keel_i64_256)(mask), (scale)))

#define _mm_i64gather_ps(base, index, scale)                                   \
  ((__m128)__builtin_keel_gather_avx2vgatherqps128(                            \
      (const void *)(base), (__keel_i64_128)(index), (__m128)((__m128){0}),    \
      (__keel_i32_128)(~(__m128i){0}), (scale)))

#define _mm_mask_i64gather_ps(src, base, index, mask, scale)                   \
  ((__m128)__builtin_keel_gather_avx2vgatherqps128(                            \
      (const void *)(base), (__keel_i64_128)(index), (__m128)(src),            \
      (__keel_i32_128)(mask), (scale)))

#define _mm256_i64gather_ps(base, index, scale)                                \
  ((__m128)__builtin_keel_gather_avx2vgatherqps256(                            \
      (const void *)(base), (__keel_i64_256)(index), (__m128)((__m128){0}),    \
      (__keel_i32_128)(~(__m128i){0}), (scale)))

#define _mm256_mask_i64gather_ps(src, base, index, mask, scale)                \
  ((__m128)__builtin_keel_gather_avx2vgatherqps256(                            \
      (const void *)(base), (__keel_i64_256)(index), (__m128)(src),            \
      (__keel_i32_128)(mask), (scale)))

#define _mm_i64gather_pd(base, index, scale)                                   \
  ((__m128d)__builtin_keel_gather_avx2vgatherqpd128(                           \
      (const void *)(base), (__keel_i64_128)(index), (__m128d)((__m128d){0}),  \
      (__keel_i64_128)(~(__m128i){0}), (scale)))

#define _mm_mask_i64gather_pd(src, base, index, mask, scale)                   \
  ((__m128d)__builtin_keel_gather_avx2vgatherqpd128(                           \
      (const void *)(base), (__keel_i64_128)(index), (__m128d)(src),           \
      (__keel_i64_128)(mask), (scale)))

#define _mm256_i64gather_pd(base, index, scale)                                \
  ((__m256d)__builtin_keel_gather_avx2vgatherqpd256(                           \
      (const void *)(base), (__keel_i64_256)(index), (__m256d)((__m256d){0}),  \
      (__keel_i64_256)(~(__m256i){0}), (scale)))

#define _mm256_mask_i64gather_pd(src, base, index, mask, scale)                \
  ((__m256d)__builtin_keel_gather_avx2vgatherqpd256(                           \
      (const void *)(base), (__keel_i64_256)(index), (__m256d)(src),           \
      (__keel_i64_256)(mask), (scale)))

#define _mm_i64gather_epi32(base, index, scale)                                \
  ((__m128i)__builtin_keel_gather_avx2vpgatherqd128(                           \
      (const void *)(base), (__keel_i64_128)(index),                           \
      (__keel_i32_128)((__m128i){0}), (__keel_i32_128)(~(__m128i){0}),         \
      (scale)))

#define _mm_mask_i64gather_epi32(src, base, index, mask, scale)                \
  ((__m128i)__builtin_keel_gather_avx2vpgatherqd128(                           \
      (const void *)(base), (__keel_i64_128)(index), (__keel_i32_128)(src),    \
      (__keel_i32_128)(mask), (scale)))

#define _mm256_i64gather_epi32(base, index, scale)                             \
  ((__m128i)__builtin_keel_gather_avx2vpgatherqd256(                           \
      (const void *)(base), (__keel_i64_256)(index),                           \
      (__keel_i32_128)((__m128i){0}), (__keel_i32_128)(~(__m128i){0}),         \
      (scale)))

#define _mm256_mask_i64gather_epi32(src, base, index, mask, scale)             \
  ((__m128i)__builtin_keel_gather_avx2vpgatherqd256(                           \
      (const void *)(base), (__keel_i64_256)(index), (__keel_i32_128)(src),    \
      (__keel_i32_128)(mask), (scale)))

#define _mm_i64gather_epi64(base, index, scale)                                \
  ((__m128i)__builtin_keel_gather_avx2vpgatherqq128(                           \
      (const void *)(base), (__keel_i64_128)(index),                           \
      (__keel_i64_128)((__m128i){0}), (__keel_i64_128)(~(__m128i){0}),         \
      (scale)))

#define _mm_mask_i64gather_epi64(src, base, index, mask, scale)                \
  ((__m128i)__builtin_keel_gather_avx2vpgatherqq128(                           \
      (const void *)(base), (__keel_i64_128)(index), (__keel_i64_128)(src),    \
      (__keel_i64_128)(mask), (scale)))

#define _mm256_i64gather_epi64(base, index, scale)                             \
  ((__m256i)__builtin_keel_gather_avx2vpgatherqq256(                           \
      (const void *)(base), (__keel_i64_256)(index),                           \
      (__keel_i64_256)((__m256i){0}), (__keel_i64_256)(~(__m256i){0}),         \
      (scale)))

#define _mm256_mask_i64gather_epi64(src, base, index, mask, scale)             \
  ((__m256i)__builtin_keel_gather_avx2vpgatherqq256(                           \
      (const void *)(base), (__keel_i64_256)(index), (__keel_i64_256)(src),    \
      (__keel_i64_256)(mask), (scale)))

#endif /* __KEEL_AVX2_MASK_MEMORY_H */
