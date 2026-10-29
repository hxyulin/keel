int printf(const char *, ...);
#include "x86_sysv_aggregates.h"
static int fails;
static void check(const char *t, const char *what, long got, long want) {
  if (got != want) {
    fails++;
    printf("FAIL %s %s: got %ld want %ld\n", t, what, got, want);
  }
}
#define RUN(T)                                                                     \
  {                                                                                \
    long k = __LINE__ % 7 + 2;                                                     \
    T s = make_##T(k);                                                             \
    long want = hash_##T(s);                                                       \
    check(#T, "return", hash_##T(mk_##T(k)), want);                                \
    check(#T, "arg", take_##T(s), want);                                           \
    check(#T, "ex", ex_##T(1.5, 2.5, 3.5, 4.5, 5.5, 6.5, 7.5, 1, 2, 3, 4, 5, s, 9.5, 6), \
          want * 1000003 + (long)(1.5 + 5 + 10.5 + 18 + 27.5 + 39 + 52.5 + 76) +   \
              1 + 4 + 9 + 16 + 25 + 36);                                           \
    check(#T, "full",                                                              \
          full_##T(1.5, 2.5, 3.5, 4.5, 5.5, 6.5, 7.5, 8.5, 1, 2, 3, 4, 5, 6, s, 9.5, 7), \
          want * 1000003 + (long)(1.5 + 5 + 10.5 + 18 + 27.5 + 39 + 52.5 + 68 + 85.5) + \
              1 + 4 + 9 + 16 + 25 + 36 + 49);                                      \
  }
int main(void) {
  TYPES(RUN)
  printf("%s: %d failures\n", fails ? "FAIL" : "ok", fails);
  return fails != 0;
}
