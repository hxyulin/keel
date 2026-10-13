#include <stdint.h>
#include <stdio.h>

int64_t k_sum(int32_t, ...);
double k_pairs(int32_t, ...);
int32_t k_printf(const char *, ...);
int64_t k_tail(int64_t, int64_t, int64_t, int64_t, int64_t, int64_t, int64_t, int64_t, int32_t,
               ...);
int64_t k_sum_three(int64_t, int64_t, int64_t);

static int failures;

#define CHECK(got, want)                                                      \
    do {                                                                      \
        double got_ = (got), want_ = (want);                                  \
        if (got_ != want_) {                                                  \
            printf("line %d: %s = %.17g, want %.17g\n", __LINE__, #got, got_, \
                   want_);                                                    \
            failures++;                                                       \
        }                                                                     \
    } while (0)

int main(void) {
    CHECK(k_sum(0), 0);
    CHECK(k_sum(1, (int64_t)5), 5);
    CHECK(k_sum(10, (int64_t)1, (int64_t)2, (int64_t)3, (int64_t)4, (int64_t)5, (int64_t)6,
                (int64_t)7, (int64_t)8, (int64_t)9, (int64_t)-10000000000),
          45 - 10000000000.0);
    CHECK(k_pairs(3, 1, 0.5, -2, 1.25, 4, 3.0), 0.5 - 2.5 + 12.0);
    int64_t target = 1000;
    CHECK(k_tail(1, 2, 3, 4, 5, 6, 7, 8, -5, &target, (int64_t)70000), -5 + 1000 + 70000 + 8);
    CHECK(k_sum_three(4, 5, -6), 3);
    CHECK(k_printf("forwarded %d and %.2f and %s\n", 42, 2.5, "keel"), 31);
    return failures;
}
