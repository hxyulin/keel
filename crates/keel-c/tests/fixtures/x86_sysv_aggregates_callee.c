#include "x86_sysv_aggregates.h"
#define DEF(T)                                                                     \
  T mk_##T(long k) { return make_##T(k); }                                         \
  long take_##T(T s) { return hash_##T(s); }                                       \
  long ex_##T(double a0, double a1, double a2, double a3, double a4, double a5,    \
              double a6, long b0, long b1, long b2, long b3, long b4, T s,         \
              double c, long d) {                                                  \
    return hash_##T(s) * 1000003 +                                                 \
           (long)(a0 + 2 * a1 + 3 * a2 + 4 * a3 + 5 * a4 + 6 * a5 + 7 * a6 + 8 * c) + \
           b0 + 2 * b1 + 3 * b2 + 4 * b3 + 5 * b4 + 6 * d;                         \
  }                                                                                \
  long full_##T(double a0, double a1, double a2, double a3, double a4, double a5,  \
                double a6, double a7, long b0, long b1, long b2, long b3, long b4, \
                long b5, T s, double c, long d) {                                  \
    return hash_##T(s) * 1000003 +                                                 \
           (long)(a0 + 2 * a1 + 3 * a2 + 4 * a3 + 5 * a4 + 6 * a5 + 7 * a6 + 8 * a7 + 9 * c) + \
           b0 + 2 * b1 + 3 * b2 + 4 * b3 + 5 * b4 + 6 * b5 + 7 * d;               \
  }
TYPES(DEF)
