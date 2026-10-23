#include <immintrin.h>

__m256 k_float(__m256 a, __m256 b) {
  __m256 c = _mm256_add_ps(a, _mm256_mul_ps(b, _mm256_set1_ps(2.0f)));
  return _mm256_shuffle_ps(c, b, _MM_SHUFFLE(2, 1, 3, 0));
}
__m256d k_double(__m256d a, __m256d b) {
  return _mm256_div_pd(_mm256_sub_pd(a, b), _mm256_set1_pd(2.0));
}
__m256i k_integer(__m256i a, __m256i b) {
  __m256i c = _mm256_mullo_epi32(_mm256_add_epi32(a, b), b);
  return _mm256_xor_si256(_mm256_shuffle_epi32(c, 27),
                          _mm256_cmpgt_epi32(a, b));
}
__m256 k_cmp(__m256 a, __m256 b) { return _mm256_cmp_ps(a, b, _CMP_NEQ_UQ); }
__m256d k_cmpd(__m256d a, __m256d b) { return _mm256_cmp_pd(a, b, _CMP_LE_OQ); }
__m256i k_cast(__m256 a) { return _mm256_castps_si256(a); }
__m256 k_half(__m256 a, __m128 b) { return _mm256_insertf128_ps(a, b, 1); }
__m128 k_extract(__m256 a) { return _mm256_extractf128_ps(a, 1); }
__m128 k_low(__m256 a) { return _mm256_castps256_ps128(a); }
__m128 k_sse(__m128 a, __m128 b) {
  return _mm_shuffle_ps(_mm_add_ps(a, b), b, 27);
}
__m128i k_sse_integer(__m128i a, __m128i b) {
  return _mm_xor_si128(_mm_mullo_epi16(a, b), _mm_cmpeq_epi8(a, b));
}
void k_copy32(void *dst, const void *src) {
  _mm256_storeu_si256((__m256i *)dst, _mm256_loadu_si256((const __m256i *)src));
}
void k_copy16(void *dst, const void *src) {
  _mm_storeu_si128((__m128i *)dst, _mm_loadu_si128((const __m128i *)src));
}
void k_copy_float(float *dst, const float *src) {
  _mm256_storeu_ps(dst, _mm256_loadu_ps(src));
}
void k_copy_double(double *dst, const double *src) {
  _mm256_storeu_pd(dst, _mm256_loadu_pd(src));
}
int k_once(__m128i a, int *count) {
  __m128i b = _mm_shuffle_epi32(((*count)++, a), 27);
  typedef int V __attribute__((vector_size(16)));
  return ((V)b)[0];
}
