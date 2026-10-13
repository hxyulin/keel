#include <stdint.h>
#include <stdio.h>

int32_t dense(int32_t);
int64_t dense_off(int64_t);
int32_t dense_big(int32_t);
int64_t sparse(int64_t);
int32_t narrow(int32_t); /* i8 in the IR; C passes the low byte */
int64_t machine(int64_t);
int32_t constant(void);

static int failures;

#define CHECK(got, want)                                                      \
    do {                                                                      \
        long long got_ = (got), want_ = (want);                               \
        if (got_ != want_) {                                                  \
            printf("line %d: %s = %lld, want %lld\n", __LINE__, #got, got_,   \
                   want_);                                                    \
            failures++;                                                       \
        }                                                                     \
    } while (0)

static int32_t model_dense(int32_t x) {
    switch (x) {
    case 0: case 2: return 10;
    case 1: case 7: return 20;
    case 4: return x + 1000;
    case 8: return 1100;
    case 5: return 50;
    default: return -1;
    }
}

static int64_t model_dense_off(int64_t x) {
    switch (x) {
    case 1000: case 1002: case 1003: return 7;
    default: return 2 * x;
    }
}

static int32_t model_dense_big(int32_t x) {
    switch (x) {
    case 70000: case 70004: return 1;
    case 70001: return 2;
    case 70002: return 3;
    case 70003: return 99;
    default: return x;
    }
}

static int64_t model_sparse(int64_t x) {
    switch (x) {
    case -1: case 3: return 11;
    case 0x123456789: return 22;
    case 5000: return 33;
    default: return 44;
    }
}

static int32_t model_narrow(int8_t x) {
    switch (x) {
    case -128: case 0: return 1;
    case -1: case 127: return 2;
    default: return 3;
    }
}

static int64_t model_machine(int64_t n) {
    int64_t state = 0, acc = 0;
    for (int64_t i = 0; i < n; i++) {
        switch (state) {
        case 0: acc += 1; state = 1; break;
        case 1: acc *= 3; state = 2; break;
        default: acc -= i + 1; state = 0; break;
        }
    }
    return acc;
}

int main(void) {
    for (int32_t x = -3; x <= 12; x++) CHECK(dense(x), model_dense(x));
    CHECK(dense(INT32_MIN), -1);
    CHECK(dense(INT32_MAX), -1);
    for (int64_t x = 995; x <= 1010; x++) CHECK(dense_off(x), model_dense_off(x));
    CHECK(dense_off(-1), -2);
    for (int32_t x = 69995; x <= 70010; x++) CHECK(dense_big(x), model_dense_big(x));
    int64_t sp[] = {-1, 0, 3, 4, 5000, 4999, 0x123456789, 0x23456789, -0x123456789};
    for (int i = 0; i < 9; i++) CHECK(sparse(sp[i]), model_sparse(sp[i]));
    for (int x = -300; x <= 300; x++) CHECK(narrow(x), model_narrow((int8_t)x));
    for (int64_t n = 0; n <= 10; n++) CHECK(machine(n), model_machine(n));
    CHECK(constant(), 1);
    /* An exit status is taken mod 256. */
    return failures != 0;
}
