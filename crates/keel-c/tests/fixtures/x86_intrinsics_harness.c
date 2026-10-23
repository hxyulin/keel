#include <immintrin.h>
extern int memcmp(const void *, const void *, unsigned long);
extern void *mmap(void *, unsigned long, int, int, int, long);
extern int mprotect(void *, unsigned long, int);
extern int munmap(void *, unsigned long);
extern int getpagesize(void);

#define DECL(ret, name, args)                                                  \
  extern ret k_##name args;                                                    \
  extern ret c_##name args;
DECL(__m256, float, (__m256, __m256))
DECL(__m256d, double, (__m256d, __m256d))
DECL(__m256i, integer, (__m256i, __m256i))
DECL(__m256, cmp, (__m256, __m256))
DECL(__m256d, cmpd, (__m256d, __m256d))
DECL(__m256i, cast, (__m256))
DECL(__m256, half, (__m256, __m128))
DECL(__m128, extract, (__m256))
DECL(__m128, low, (__m256))
DECL(__m128, sse, (__m128, __m128))
DECL(__m128i, sse_integer, (__m128i, __m128i))
extern int k_once(__m128i, int *);
extern void k_copy32(void *, const void *), k_copy16(void *, const void *);
extern void k_copy_float(float *, const float *),
    k_copy_double(double *, const double *);

#define CHECK(type, name, ...)                                                 \
  do {                                                                         \
    type k = k_##name(__VA_ARGS__), c = c_##name(__VA_ARGS__);                 \
    if (memcmp(&k, &c, sizeof(k)))                                             \
      return __LINE__;                                                         \
  } while (0)
int intrinsic_checks(void) {
  __m256 a = {1, 2, 3, 4, 5, 6, 7, 8}, b = {9, 10, 11, 12, 13, 14, 15, 16};
  __m256d d = {1, 2, 3, 4}, e = {5, 6, 7, 8};
  typedef int I __attribute__((vector_size(32)));
  typedef int J __attribute__((vector_size(16)));
  I x = {-7, 9, -100, 0, 42, 500, 6, -88}, y = {3, 7, 100, 0, 6, 77, -1, 2};
  CHECK(__m256, float, a, b);
  CHECK(__m256d, double, d, e);
  CHECK(__m256i, integer, (__m256i)x, (__m256i)y);
  CHECK(__m256i, cast, a);
  CHECK(__m256, half, a, (__m128){40, 30, 20, 10});
  CHECK(__m128, extract, a);
  CHECK(__m128, low, a);
  CHECK(__m128, sse, (__m128){1, 2, 3, 4}, (__m128){5, 6, 7, 8});
  CHECK(__m128i, sse_integer, (__m128i)(J){-1, 2, 3, 4},
        (__m128i)(J){5, 6, 7, 8});
  int count = 0;
  if (k_once((__m128i)(J){1, 2, 3, 4}, &count) != 4 || count != 1)
    return __LINE__;
  a[0] = __builtin_nanf("");
  a[1] = -0.0f;
  b[1] = 0.0f;
  d[0] = __builtin_nan("");
  d[1] = -0.0;
  e[1] = 0.0;
  CHECK(__m256, cmp, a, b);
  CHECK(__m256d, cmpd, d, e);
  unsigned long page = (unsigned long)getpagesize();
  unsigned char *src = mmap(0, page * 3, 0, 0x22, -1, 0),
                *dst = mmap(0, page * 3, 0, 0x22, -1, 0);
  if (src == (void *)-1 || dst == (void *)-1)
    return __LINE__;
  if (mprotect(src + page, page, 3) || mprotect(dst + page, page, 3))
    return __LINE__;
  /* Protected pages on both sides, plus byte sentinels around each exact-width
   * store. */
  for (int width = 16; width <= 32; width += 16)
    for (int end = 0; end < 2; end++)
      for (int skew = 0; skew < 2; skew++) {
        unsigned long offset =
            end ? page * 2 - (unsigned long)width - (unsigned long)skew
                : page + (unsigned long)skew;
        for (unsigned long i = page; i < page * 2; i++) {
          src[i] = (unsigned char)(i * 13);
          dst[i] = 0xA5;
        }
        if (width == 16)
          k_copy16(dst + offset, src + offset);
        else
          k_copy32(dst + offset, src + offset);
        if (memcmp(dst + offset, src + offset, (unsigned long)width))
          return __LINE__;
        for (unsigned long i = page; i < page * 2; i++)
          if ((i < offset || i >= offset + (unsigned long)width) &&
              dst[i] != 0xA5)
            return __LINE__;
      }
  for (unsigned long i = page; i < page * 2; i++)
    dst[i] = 0;
  k_copy_float((float *)(dst + page * 2 - 32),
               (const float *)(src + page * 2 - 32));
  if (memcmp(dst + page * 2 - 32, src + page * 2 - 32, 32))
    return __LINE__;
  k_copy_double((double *)(dst + page), (const double *)(src + page));
  if (memcmp(dst + page, src + page, 32))
    return __LINE__;
  munmap(src, page * 3);
  munmap(dst, page * 3);
  return 0;
}
