/* Inactive lanes suppress memory accesses; architectural builtins own effects.
 */
#ifndef __KEEL_AVX_MASK_MEMORY_H
#define __KEEL_AVX_MASK_MEMORY_H

static __inline__ __attribute__((always_inline)) __m128 _mm_maskload_ps(const float *p, __m128i mask) {
  return (__m128)__builtin_keel_mask_avxvmaskmovpsload128((const void *)p,
                                                          (__keel_i32_128)mask);
}

static __inline__ __attribute__((always_inline)) void _mm_maskstore_ps(float *p, __m128i mask, __m128 data) {
  __builtin_keel_mask_avxvmaskmovpsstore128((void *)p, (__m128)data,
                                            (__keel_i32_128)mask);
}

static __inline__ __attribute__((always_inline)) __m256 _mm256_maskload_ps(const float *p, __m256i mask) {
  return (__m256)__builtin_keel_mask_avxvmaskmovpsload256((const void *)p,
                                                          (__keel_i32_256)mask);
}

static __inline__ __attribute__((always_inline)) void _mm256_maskstore_ps(float *p, __m256i mask,
                                           __m256 data) {
  __builtin_keel_mask_avxvmaskmovpsstore256((void *)p, (__m256)data,
                                            (__keel_i32_256)mask);
}

static __inline__ __attribute__((always_inline)) __m128d _mm_maskload_pd(const double *p, __m128i mask) {
  return (__m128d)__builtin_keel_mask_avxvmaskmovpdload128(
      (const void *)p, (__keel_i64_128)mask);
}

static __inline__ __attribute__((always_inline)) void _mm_maskstore_pd(double *p, __m128i mask, __m128d data) {
  __builtin_keel_mask_avxvmaskmovpdstore128((void *)p, (__m128d)data,
                                            (__keel_i64_128)mask);
}

static __inline__ __attribute__((always_inline)) __m256d _mm256_maskload_pd(const double *p, __m256i mask) {
  return (__m256d)__builtin_keel_mask_avxvmaskmovpdload256((const void *)p,
                                                           (__m256i)mask);
}

static __inline__ __attribute__((always_inline)) void _mm256_maskstore_pd(double *p, __m256i mask,
                                           __m256d data) {
  __builtin_keel_mask_avxvmaskmovpdstore256((void *)p, (__m256d)data,
                                            (__m256i)mask);
}

#endif /* __KEEL_AVX_MASK_MEMORY_H */
