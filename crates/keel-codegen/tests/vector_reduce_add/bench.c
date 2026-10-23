#define _GNU_SOURCE
#include <stdint.h>
#include <stdio.h>
#include <time.h>

typedef uint64_t V64 __attribute__((vector_size(16)));
typedef uint32_t V32 __attribute__((vector_size(16)));
extern uint64_t sum_i64x2(V64);
extern uint32_t sum_i32x4(V32);
V64 opaque_i64x2(V64 x) { return x; }
V32 opaque_i32x4(V32 x) { return x; }

static uint64_t ns(void) {
    struct timespec t;
    clock_gettime(CLOCK_MONOTONIC_RAW, &t);
    return (uint64_t)t.tv_sec * 1000000000 + t.tv_nsec;
}

int main(void) {
    uint64_t acc = 0, start = ns();
    for (unsigned i = 0; i < 10000000; i++) {
        V64 x = {(uint64_t)i * 0x123456789abcdefULL, ~(uint64_t)i};
        acc += sum_i64x2(x);
    }
    uint64_t elapsed = ns() - start;
    printf("i64x2 %.4f %llu\n", elapsed / 10000000., (unsigned long long)acc);
    acc = 0;
    start = ns();
    for (unsigned i = 0; i < 10000000; i++) {
        V32 x = {i, i * 3, ~i, i * 7};
        acc += sum_i32x4(x);
    }
    elapsed = ns() - start;
    printf("i32x4 %.4f %llu\n", elapsed / 10000000., (unsigned long long)acc);
}
