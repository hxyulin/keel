#include <stdint.h>
#include <stdio.h>
#include <unistd.h>

int64_t k_add64(int64_t a, int64_t b);
uint32_t k_rev32(uint32_t x);
int64_t k_addw(int64_t a, int64_t b);
double k_fmadd(double a, double b, double c);
float k_fadd32(float a, float b);
float k_fcvt(double x);
int64_t k_load(const int64_t *p);
int64_t k_add42(int64_t x);
int64_t k_tied(int64_t a, int64_t b);
int64_t k_getpid(void);
int32_t k_add_carry(uint64_t a, uint64_t b, uint64_t *sum);
int64_t k_clobber_all(int64_t x);
int64_t k_clobber_caller(int64_t x);

/* x19-x28, d8-d15 as k_clobber_all left them, then its result. */
int64_t saved[19];

/* Calls k_clobber_all(5) with sentinels in every callee-saved register and
 * records them afterwards: it must restore all that its assembly zeroes. */
static void call_with_sentinels(void) {
    __asm__ volatile(
        "mov x19, #19\n\tmov x20, #20\n\tmov x21, #21\n\tmov x22, #22\n\t"
        "mov x23, #23\n\tmov x24, #24\n\tmov x25, #25\n\tmov x26, #26\n\t"
        "mov x27, #27\n\tmov x28, #28\n\t"
        "fmov d8, #8.0\n\tfmov d9, #9.0\n\tfmov d10, #10.0\n\tfmov d11, #11.0\n\t"
        "fmov d12, #12.0\n\tfmov d13, #13.0\n\tfmov d14, #14.0\n\tfmov d15, #15.0\n\t"
        "mov x0, #5\n\t"
        "bl _k_clobber_all\n\t"
        "adrp x9, _saved@PAGE\n\tadd x9, x9, _saved@PAGEOFF\n\t"
        "stp x19, x20, [x9]\n\tstp x21, x22, [x9, #16]\n\tstp x23, x24, [x9, #32]\n\t"
        "stp x25, x26, [x9, #48]\n\tstp x27, x28, [x9, #64]\n\t"
        "stp d8, d9, [x9, #80]\n\tstp d10, d11, [x9, #96]\n\t"
        "stp d12, d13, [x9, #112]\n\tstp d14, d15, [x9, #128]\n\t"
        "str x0, [x9, #144]"
        :
        :
        : "x0", "x1", "x2", "x3", "x4", "x5", "x6", "x7", "x8", "x9", "x10", "x11", "x12",
          "x13", "x14", "x15", "x16", "x17", "x19", "x20", "x21", "x22", "x23", "x24", "x25",
          "x26", "x27", "x28", "x30", "d0", "d1", "d2", "d3", "d4", "d5", "d6", "d7", "d8",
          "d9", "d10", "d11", "d12", "d13", "d14", "d15", "d16", "d17", "d18", "d19", "d20",
          "d21", "d22", "d23", "d24", "d25", "d26", "d27", "d28", "d29", "d30", "d31", "memory",
          "cc");
}

#define CHECK(cond)                            \
    do {                                       \
        if (!(cond)) {                         \
            printf("failed: %s\n", #cond);     \
            failures++;                        \
        }                                      \
    } while (0)

int main(void) {
    int failures = 0;
    CHECK(k_add64(40, 2) == 42);
    CHECK(k_add64(INT64_MAX, 1) == INT64_MIN);
    CHECK(k_rev32(0x11223344u) == 0x44332211u);
    CHECK(k_addw(0x100000005, 7) == 12);
    CHECK(k_addw(0xffffffff, 1) == 0);
    CHECK(k_fmadd(2.0, 3.0, 0.5) == 6.5);
    CHECK(k_fadd32(1.25f, 2.5f) == 3.75f);
    CHECK(k_fcvt(1.5) == 1.5f);
    int64_t x = -77;
    CHECK(k_load(&x) == -77);
    CHECK(k_add42(-2) == 40);
    CHECK(k_tied(30, 12) == 42);
    CHECK(k_getpid() == getpid());
    CHECK(k_clobber_caller(41) == 42);
    uint64_t sum = 0;
    CHECK(k_add_carry(UINT64_MAX, 2, &sum) == 1 && sum == 1);
    CHECK(k_add_carry(5, 6, &sum) == 0 && sum == 11);

    call_with_sentinels();
    for (int i = 0; i < 10; i++) {
        if (saved[i] != 19 + i) {
            printf("x%d = %lld after k_clobber_all\n", 19 + i, (long long)saved[i]);
            failures++;
        }
    }
    for (int i = 0; i < 8; i++) {
        double d;
        __builtin_memcpy(&d, &saved[10 + i], sizeof d);
        if (d != 8 + i) {
            printf("d%d = %g after k_clobber_all\n", 8 + i, d);
            failures++;
        }
    }
    CHECK(saved[18] == 28);
    return failures;
}
