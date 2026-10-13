#include <stdint.h>
#include <stdio.h>

int64_t rotate3(int64_t, int64_t, int64_t, int64_t);
int64_t rotate_many(int64_t, int64_t);
int64_t invariant(int64_t, int64_t);

static int64_t rotate_many_model(int64_t x, int64_t n) {
    uint64_t h = 0;
    for (int64_t i = 0; i < 20; i++) h = h * 31 + (uint64_t)x + (uint64_t)((i + n) % 20);
    return (int64_t)h;
}

static int64_t invariant_model(int64_t x, int64_t n) {
    uint64_t k1 = (uint64_t)x * 3, k2 = (uint64_t)x + 7, acc = 0;
    for (uint64_t m = (uint64_t)n; m != 0; m--) {
        uint64_t t = (m - 1) * 5 + 11;
        acc = ((acc + k1) ^ k2) + (t ^ 13) * t;
    }
    return (int64_t)acc;
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
    CHECK(rotate3(1, 2, 3, 0), 123);
    CHECK(rotate3(1, 2, 3, 1), 231);
    CHECK(rotate3(1, 2, 3, 2), 312);
    CHECK(rotate3(1, 2, 3, 3), 123);
    CHECK(rotate3(1, 2, 3, 7), 231);
    int64_t ns[] = {0, 1, 7, 20, 23};
    for (int i = 0; i < 5; i++) {
        CHECK(rotate_many(1000, ns[i]), rotate_many_model(1000, ns[i]));
        CHECK(rotate_many(-3, ns[i]), rotate_many_model(-3, ns[i]));
        CHECK(invariant(ns[i], ns[i]), invariant_model(ns[i], ns[i]));
        CHECK(invariant(-ns[i] - 1000, 9), invariant_model(-ns[i] - 1000, 9));
    }
    return failures;
}
