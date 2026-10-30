#include <immintrin.h>
void cpu_initialize(void) { __builtin_cpu_init(); }
#define QUERY(suffix, name) int query_##suffix(void) { return __builtin_cpu_supports(name); }
QUERY(monitor, "monitor")
QUERY(sse, "sse")
QUERY(sse2, "sse2")
QUERY(sse3, "sse3")
QUERY(ssse3, "ssse3")
QUERY(sse41, "sse4.1")
QUERY(sse42, "sse4.2")
QUERY(avx, "avx")
QUERY(avx2, "avx2")
QUERY(avx512f, "avx512f")
QUERY(avx512vl, "avx512vl")
QUERY(aes, "aes")
QUERY(pclmul, "pclmul")
QUERY(popcnt, "popcnt")
QUERY(bmi, "bmi")
QUERY(bmi2, "bmi2")
QUERY(cx16, "cx16")
QUERY(movbe, "movbe")
QUERY(lzcnt, "lzcnt")
QUERY(xsave, "xsave")
QUERY(fma, "fma")
QUERY(f16c, "f16c")
__attribute__((target("avx2"), noinline))
static void accelerated(int *out, const int *a, const int *b) {
  _mm256_storeu_si256((__m256i *)out,
    _mm256_add_epi32(_mm256_loadu_si256((const __m256i *)a),
                    _mm256_loadu_si256((const __m256i *)b)));
}
int dispatch_add(int *out, const int *a, const int *b) {
  if (__builtin_cpu_supports("avx2")) { accelerated(out,a,b); return 1; }
  for (unsigned i=0;i<8;++i) out[i]=a[i]+b[i];
  return 0;
}
