#include <stdio.h>
#include <stdlib.h>
typedef __UINT64_TYPE__ U;
static U *mutate;
static unsigned calls;
U callback(U x) {
  calls++;
  if (mutate) for (unsigned i = 0; i < 12; i++) mutate[i] ^= ~(U)i;
  return (x ^ 0x9e3779b97f4a7c15ULL) + 1;
}
#define DECL(P)                                                                \
  U P##across(const U *, U);                                                   \
  U P##cold_call(U, unsigned);                                                 \
  U P##dispatch(const unsigned char *, const U *, unsigned);                   \
  U P##pressure(const U *, unsigned);                                          \
  U P##branch_loop(const U *, unsigned, U);
DECL(keel_)
DECL(clang_)
DECL(hand_)

static unsigned char ops[4096];
static U data[4096];
static U rand64(U *s) {
  *s ^= *s << 13;
  *s ^= *s >> 7;
  *s ^= *s << 17;
  return *s;
}
#define CHECK(V, W)                                                            \
  do {                                                                         \
    U a = (V), b = (W);                                                        \
    if (a != b) {                                                              \
      fprintf(stderr, "%s: %llx != %llx\n", #V, (unsigned long long)a,         \
              (unsigned long long)b);                                          \
      exit(1);                                                                 \
    }                                                                          \
  } while (0)
int main(void) {
  U seed = 123456789;
  for (unsigned i = 0; i < 4096; i++) {
    data[i] = rand64(&seed);
    ops[i] = rand64(&seed);
  }
  {
    const unsigned lengths[] = {0, 1, 2, 7, 31, 257};
    for (unsigned k = 0; k < sizeof(lengths) / sizeof(lengths[0]); k++) {
      unsigned n = lengths[k];
      for (unsigned pattern = 0; pattern < 9; pattern++) {
        for (unsigned i = 0; i < n; i++)
          ops[i] = pattern < 8 ? pattern : rand64(&seed);
#define TEST(P)                                                                \
  CHECK(P##dispatch(ops, data, n), clang_dispatch(ops, data, n));              \
  CHECK(P##pressure(data, n), clang_pressure(data, n));                        \
  CHECK(P##branch_loop(data, n, 15), clang_branch_loop(data, n, 15));
        TEST(keel_) TEST(hand_)
      }
      for (unsigned f = 0; f < 2; f++) {
        U x = rand64(&seed);
        CHECK(keel_cold_call(x, f), clang_cold_call(x, f));
        CHECK(hand_cold_call(x, f), clang_cold_call(x, f));
      }
    }
    U (*functions[])(const U *, U) = {keel_across, clang_across, hand_across};
    for (unsigned trial = 0; trial < 100; trial++) {
      U original[12], expected = 0, argument = rand64(&seed);
      for (unsigned i = 0; i < 12; i++) expected += original[i] = rand64(&seed);
      expected += (argument ^ 0x9e3779b97f4a7c15ULL) + 1;
      for (unsigned f = 0; f < 3; f++) {
        U input[12];
        for (unsigned i = 0; i < 12; i++) input[i] = original[i];
        mutate = input; calls = 0;
        CHECK(functions[f](input, argument), expected);
        if (calls != 1) return 2;
        for (unsigned i = 0; i < 12; i++) if (input[i] != (original[i] ^ ~(U)i)) return 3;
        mutate = 0;
      }
    }
    puts("all kernel oracles passed");
    return 0;
  }
}
