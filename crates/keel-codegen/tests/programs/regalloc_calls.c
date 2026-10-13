#include <stdint.h>
#include <stdio.h>

#include "clobber.h"

int64_t across(int64_t);
int64_t nested(int64_t);
int64_t call_loop(int64_t, int64_t);
int64_t memset_loop(int64_t);

int64_t twice(int64_t v) {
    clobber();
    return 2 * v;
}

static int64_t across_model(int64_t x) {
    uint64_t u = (uint64_t)x;
    uint64_t a = u * 3, b = u + 5;
    int64_t c = (int32_t)((uint32_t)u + 7u);
    return (int64_t)(a + b + (a ^ b) + (uint64_t)c + u);
}

static int failures;

#define CHECK(expr, want)                                                     \
    do {                                                                      \
        long long got_ = (long long)(expr), want_ = (long long)(want);        \
        if (got_ != want_) {                                                  \
            printf("%s = %lld, want %lld\n", #expr, got_, want_);             \
            failures++;                                                       \
        }                                                                     \
    } while (0)

int main(void) {
    int64_t xs[] = {0, 1, -9, 0x7fffffff, 1LL << 40};
    for (int i = 0; i < 5; i++) {
        int64_t x = xs[i];
        CHECK(across(x), across_model(x));
        CHECK(nested(x), across_model(x) + across_model(x + 1) + (x + 1) + 2 * x + x);
    }
    for (int64_t n = 0; n <= 5; n++) {
        int64_t want = 0;
        for (int64_t i = 0; i < n; i++) want += 2 * i + 10 * 7;
        CHECK(call_loop(n, 7), want);
    }
    for (int64_t n = 0; n <= 300; n += 100) {
        uint64_t want = 0;
        for (int64_t i = 0; i < n; i++) want += (uint64_t)(i & 0xff) * 0x0101010101010101ull;
        CHECK(memset_loop(n), want);
    }
    /* An exit status is taken mod 256. */
    return failures != 0;
}
