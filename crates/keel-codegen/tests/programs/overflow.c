#include <stdint.h>
#include <stdio.h>
#include "clobber.h"

typedef __int128 i128;
typedef unsigned __int128 u128;

uint8_t k_uadd_i8(uint8_t a, uint8_t b, uint8_t *flag);
int8_t k_sadd_i8(int8_t a, int8_t b, uint8_t *flag);
uint8_t k_usub_i8(uint8_t a, uint8_t b, uint8_t *flag);
int8_t k_ssub_i8(int8_t a, int8_t b, uint8_t *flag);
uint8_t k_umul_i8(uint8_t a, uint8_t b, uint8_t *flag);
int8_t k_smul_i8(int8_t a, int8_t b, uint8_t *flag);
uint16_t k_uadd_i16(uint16_t a, uint16_t b, uint8_t *flag);
int16_t k_sadd_i16(int16_t a, int16_t b, uint8_t *flag);
uint16_t k_usub_i16(uint16_t a, uint16_t b, uint8_t *flag);
int16_t k_ssub_i16(int16_t a, int16_t b, uint8_t *flag);
uint16_t k_umul_i16(uint16_t a, uint16_t b, uint8_t *flag);
int16_t k_smul_i16(int16_t a, int16_t b, uint8_t *flag);
uint32_t k_uadd_i32(uint32_t a, uint32_t b, uint8_t *flag);
int32_t k_sadd_i32(int32_t a, int32_t b, uint8_t *flag);
uint32_t k_usub_i32(uint32_t a, uint32_t b, uint8_t *flag);
int32_t k_ssub_i32(int32_t a, int32_t b, uint8_t *flag);
uint32_t k_umul_i32(uint32_t a, uint32_t b, uint8_t *flag);
int32_t k_smul_i32(int32_t a, int32_t b, uint8_t *flag);
uint64_t k_uadd_i64(uint64_t a, uint64_t b, uint8_t *flag);
int64_t k_sadd_i64(int64_t a, int64_t b, uint8_t *flag);
uint64_t k_usub_i64(uint64_t a, uint64_t b, uint8_t *flag);
int64_t k_ssub_i64(int64_t a, int64_t b, uint8_t *flag);
uint64_t k_umul_i64(uint64_t a, uint64_t b, uint8_t *flag);
int64_t k_smul_i64(int64_t a, int64_t b, uint8_t *flag);
u128 k_uadd_i128(u128 a, u128 b, uint8_t *flag);
i128 k_sadd_i128(i128 a, i128 b, uint8_t *flag);
u128 k_usub_i128(u128 a, u128 b, uint8_t *flag);
i128 k_ssub_i128(i128 a, i128 b, uint8_t *flag);
int64_t k_live(int64_t a, int64_t b);

static int failures;

/* Edge values of every width, as 128-bit patterns truncated to each type. */
static const u128 edges[] = {
    0, 1, 2, 3, 0x7f, 0x80, 0xff, 0x100, 0x7fff, 0x8000, 0xffff, 0x10000,
    0x7fffffff, 0x80000000, 0xffffffff, 0x100000000,
    0x7fffffffffffffff, 0x8000000000000000, 0xffffffffffffffff,
    0x1234567, 0xdeadbeef, 0x10001,
};
#define N (sizeof edges / sizeof edges[0])

static u128 edge(int i) {
    /* The negations too, and 128-bit extremes. */
    if (i < (int)N) return edges[i];
    if (i < 2 * (int)N) return -edges[i - N];
    if (i == 2 * (int)N) return (u128)1 << 127;
    return ~((u128)1 << 127);
}
#define COUNT (2 * (int)N + 2)

static void check_uadd_i8(void) {
    for (int i = 0; i < COUNT; i++)
        for (int j = 0; j < COUNT; j++) {
            uint8_t a = (uint8_t)edge(i), b = (uint8_t)edge(j), want;
            int want_flag = __builtin_add_overflow(a, b, &want);
            uint8_t flag = 7;
            uint8_t got = k_uadd_i8(a, b, &flag);
            if (got != want || flag != want_flag) {
                if (failures++ < 20)
                    printf("uadd_overflow.i8(%#llx, %#llx) = %#llx flag %d, want %#llx flag %d\n",
                           (unsigned long long)a, (unsigned long long)b, (unsigned long long)got, flag,
                           (unsigned long long)want, want_flag);
            }
        }
}

static void check_sadd_i8(void) {
    for (int i = 0; i < COUNT; i++)
        for (int j = 0; j < COUNT; j++) {
            int8_t a = (int8_t)edge(i), b = (int8_t)edge(j), want;
            int want_flag = __builtin_add_overflow(a, b, &want);
            uint8_t flag = 7;
            int8_t got = k_sadd_i8(a, b, &flag);
            if (got != want || flag != want_flag) {
                if (failures++ < 20)
                    printf("sadd_overflow.i8(%#llx, %#llx) = %#llx flag %d, want %#llx flag %d\n",
                           (unsigned long long)a, (unsigned long long)b, (unsigned long long)got, flag,
                           (unsigned long long)want, want_flag);
            }
        }
}

static void check_usub_i8(void) {
    for (int i = 0; i < COUNT; i++)
        for (int j = 0; j < COUNT; j++) {
            uint8_t a = (uint8_t)edge(i), b = (uint8_t)edge(j), want;
            int want_flag = __builtin_sub_overflow(a, b, &want);
            uint8_t flag = 7;
            uint8_t got = k_usub_i8(a, b, &flag);
            if (got != want || flag != want_flag) {
                if (failures++ < 20)
                    printf("usub_overflow.i8(%#llx, %#llx) = %#llx flag %d, want %#llx flag %d\n",
                           (unsigned long long)a, (unsigned long long)b, (unsigned long long)got, flag,
                           (unsigned long long)want, want_flag);
            }
        }
}

static void check_ssub_i8(void) {
    for (int i = 0; i < COUNT; i++)
        for (int j = 0; j < COUNT; j++) {
            int8_t a = (int8_t)edge(i), b = (int8_t)edge(j), want;
            int want_flag = __builtin_sub_overflow(a, b, &want);
            uint8_t flag = 7;
            int8_t got = k_ssub_i8(a, b, &flag);
            if (got != want || flag != want_flag) {
                if (failures++ < 20)
                    printf("ssub_overflow.i8(%#llx, %#llx) = %#llx flag %d, want %#llx flag %d\n",
                           (unsigned long long)a, (unsigned long long)b, (unsigned long long)got, flag,
                           (unsigned long long)want, want_flag);
            }
        }
}

static void check_umul_i8(void) {
    for (int i = 0; i < COUNT; i++)
        for (int j = 0; j < COUNT; j++) {
            uint8_t a = (uint8_t)edge(i), b = (uint8_t)edge(j), want;
            int want_flag = __builtin_mul_overflow(a, b, &want);
            uint8_t flag = 7;
            uint8_t got = k_umul_i8(a, b, &flag);
            if (got != want || flag != want_flag) {
                if (failures++ < 20)
                    printf("umul_overflow.i8(%#llx, %#llx) = %#llx flag %d, want %#llx flag %d\n",
                           (unsigned long long)a, (unsigned long long)b, (unsigned long long)got, flag,
                           (unsigned long long)want, want_flag);
            }
        }
}

static void check_smul_i8(void) {
    for (int i = 0; i < COUNT; i++)
        for (int j = 0; j < COUNT; j++) {
            int8_t a = (int8_t)edge(i), b = (int8_t)edge(j), want;
            int want_flag = __builtin_mul_overflow(a, b, &want);
            uint8_t flag = 7;
            int8_t got = k_smul_i8(a, b, &flag);
            if (got != want || flag != want_flag) {
                if (failures++ < 20)
                    printf("smul_overflow.i8(%#llx, %#llx) = %#llx flag %d, want %#llx flag %d\n",
                           (unsigned long long)a, (unsigned long long)b, (unsigned long long)got, flag,
                           (unsigned long long)want, want_flag);
            }
        }
}

static void check_uadd_i16(void) {
    for (int i = 0; i < COUNT; i++)
        for (int j = 0; j < COUNT; j++) {
            uint16_t a = (uint16_t)edge(i), b = (uint16_t)edge(j), want;
            int want_flag = __builtin_add_overflow(a, b, &want);
            uint8_t flag = 7;
            uint16_t got = k_uadd_i16(a, b, &flag);
            if (got != want || flag != want_flag) {
                if (failures++ < 20)
                    printf("uadd_overflow.i16(%#llx, %#llx) = %#llx flag %d, want %#llx flag %d\n",
                           (unsigned long long)a, (unsigned long long)b, (unsigned long long)got, flag,
                           (unsigned long long)want, want_flag);
            }
        }
}

static void check_sadd_i16(void) {
    for (int i = 0; i < COUNT; i++)
        for (int j = 0; j < COUNT; j++) {
            int16_t a = (int16_t)edge(i), b = (int16_t)edge(j), want;
            int want_flag = __builtin_add_overflow(a, b, &want);
            uint8_t flag = 7;
            int16_t got = k_sadd_i16(a, b, &flag);
            if (got != want || flag != want_flag) {
                if (failures++ < 20)
                    printf("sadd_overflow.i16(%#llx, %#llx) = %#llx flag %d, want %#llx flag %d\n",
                           (unsigned long long)a, (unsigned long long)b, (unsigned long long)got, flag,
                           (unsigned long long)want, want_flag);
            }
        }
}

static void check_usub_i16(void) {
    for (int i = 0; i < COUNT; i++)
        for (int j = 0; j < COUNT; j++) {
            uint16_t a = (uint16_t)edge(i), b = (uint16_t)edge(j), want;
            int want_flag = __builtin_sub_overflow(a, b, &want);
            uint8_t flag = 7;
            uint16_t got = k_usub_i16(a, b, &flag);
            if (got != want || flag != want_flag) {
                if (failures++ < 20)
                    printf("usub_overflow.i16(%#llx, %#llx) = %#llx flag %d, want %#llx flag %d\n",
                           (unsigned long long)a, (unsigned long long)b, (unsigned long long)got, flag,
                           (unsigned long long)want, want_flag);
            }
        }
}

static void check_ssub_i16(void) {
    for (int i = 0; i < COUNT; i++)
        for (int j = 0; j < COUNT; j++) {
            int16_t a = (int16_t)edge(i), b = (int16_t)edge(j), want;
            int want_flag = __builtin_sub_overflow(a, b, &want);
            uint8_t flag = 7;
            int16_t got = k_ssub_i16(a, b, &flag);
            if (got != want || flag != want_flag) {
                if (failures++ < 20)
                    printf("ssub_overflow.i16(%#llx, %#llx) = %#llx flag %d, want %#llx flag %d\n",
                           (unsigned long long)a, (unsigned long long)b, (unsigned long long)got, flag,
                           (unsigned long long)want, want_flag);
            }
        }
}

static void check_umul_i16(void) {
    for (int i = 0; i < COUNT; i++)
        for (int j = 0; j < COUNT; j++) {
            uint16_t a = (uint16_t)edge(i), b = (uint16_t)edge(j), want;
            int want_flag = __builtin_mul_overflow(a, b, &want);
            uint8_t flag = 7;
            uint16_t got = k_umul_i16(a, b, &flag);
            if (got != want || flag != want_flag) {
                if (failures++ < 20)
                    printf("umul_overflow.i16(%#llx, %#llx) = %#llx flag %d, want %#llx flag %d\n",
                           (unsigned long long)a, (unsigned long long)b, (unsigned long long)got, flag,
                           (unsigned long long)want, want_flag);
            }
        }
}

static void check_smul_i16(void) {
    for (int i = 0; i < COUNT; i++)
        for (int j = 0; j < COUNT; j++) {
            int16_t a = (int16_t)edge(i), b = (int16_t)edge(j), want;
            int want_flag = __builtin_mul_overflow(a, b, &want);
            uint8_t flag = 7;
            int16_t got = k_smul_i16(a, b, &flag);
            if (got != want || flag != want_flag) {
                if (failures++ < 20)
                    printf("smul_overflow.i16(%#llx, %#llx) = %#llx flag %d, want %#llx flag %d\n",
                           (unsigned long long)a, (unsigned long long)b, (unsigned long long)got, flag,
                           (unsigned long long)want, want_flag);
            }
        }
}

static void check_uadd_i32(void) {
    for (int i = 0; i < COUNT; i++)
        for (int j = 0; j < COUNT; j++) {
            uint32_t a = (uint32_t)edge(i), b = (uint32_t)edge(j), want;
            int want_flag = __builtin_add_overflow(a, b, &want);
            uint8_t flag = 7;
            uint32_t got = k_uadd_i32(a, b, &flag);
            if (got != want || flag != want_flag) {
                if (failures++ < 20)
                    printf("uadd_overflow.i32(%#llx, %#llx) = %#llx flag %d, want %#llx flag %d\n",
                           (unsigned long long)a, (unsigned long long)b, (unsigned long long)got, flag,
                           (unsigned long long)want, want_flag);
            }
        }
}

static void check_sadd_i32(void) {
    for (int i = 0; i < COUNT; i++)
        for (int j = 0; j < COUNT; j++) {
            int32_t a = (int32_t)edge(i), b = (int32_t)edge(j), want;
            int want_flag = __builtin_add_overflow(a, b, &want);
            uint8_t flag = 7;
            int32_t got = k_sadd_i32(a, b, &flag);
            if (got != want || flag != want_flag) {
                if (failures++ < 20)
                    printf("sadd_overflow.i32(%#llx, %#llx) = %#llx flag %d, want %#llx flag %d\n",
                           (unsigned long long)a, (unsigned long long)b, (unsigned long long)got, flag,
                           (unsigned long long)want, want_flag);
            }
        }
}

static void check_usub_i32(void) {
    for (int i = 0; i < COUNT; i++)
        for (int j = 0; j < COUNT; j++) {
            uint32_t a = (uint32_t)edge(i), b = (uint32_t)edge(j), want;
            int want_flag = __builtin_sub_overflow(a, b, &want);
            uint8_t flag = 7;
            uint32_t got = k_usub_i32(a, b, &flag);
            if (got != want || flag != want_flag) {
                if (failures++ < 20)
                    printf("usub_overflow.i32(%#llx, %#llx) = %#llx flag %d, want %#llx flag %d\n",
                           (unsigned long long)a, (unsigned long long)b, (unsigned long long)got, flag,
                           (unsigned long long)want, want_flag);
            }
        }
}

static void check_ssub_i32(void) {
    for (int i = 0; i < COUNT; i++)
        for (int j = 0; j < COUNT; j++) {
            int32_t a = (int32_t)edge(i), b = (int32_t)edge(j), want;
            int want_flag = __builtin_sub_overflow(a, b, &want);
            uint8_t flag = 7;
            int32_t got = k_ssub_i32(a, b, &flag);
            if (got != want || flag != want_flag) {
                if (failures++ < 20)
                    printf("ssub_overflow.i32(%#llx, %#llx) = %#llx flag %d, want %#llx flag %d\n",
                           (unsigned long long)a, (unsigned long long)b, (unsigned long long)got, flag,
                           (unsigned long long)want, want_flag);
            }
        }
}

static void check_umul_i32(void) {
    for (int i = 0; i < COUNT; i++)
        for (int j = 0; j < COUNT; j++) {
            uint32_t a = (uint32_t)edge(i), b = (uint32_t)edge(j), want;
            int want_flag = __builtin_mul_overflow(a, b, &want);
            uint8_t flag = 7;
            uint32_t got = k_umul_i32(a, b, &flag);
            if (got != want || flag != want_flag) {
                if (failures++ < 20)
                    printf("umul_overflow.i32(%#llx, %#llx) = %#llx flag %d, want %#llx flag %d\n",
                           (unsigned long long)a, (unsigned long long)b, (unsigned long long)got, flag,
                           (unsigned long long)want, want_flag);
            }
        }
}

static void check_smul_i32(void) {
    for (int i = 0; i < COUNT; i++)
        for (int j = 0; j < COUNT; j++) {
            int32_t a = (int32_t)edge(i), b = (int32_t)edge(j), want;
            int want_flag = __builtin_mul_overflow(a, b, &want);
            uint8_t flag = 7;
            int32_t got = k_smul_i32(a, b, &flag);
            if (got != want || flag != want_flag) {
                if (failures++ < 20)
                    printf("smul_overflow.i32(%#llx, %#llx) = %#llx flag %d, want %#llx flag %d\n",
                           (unsigned long long)a, (unsigned long long)b, (unsigned long long)got, flag,
                           (unsigned long long)want, want_flag);
            }
        }
}

static void check_uadd_i64(void) {
    for (int i = 0; i < COUNT; i++)
        for (int j = 0; j < COUNT; j++) {
            uint64_t a = (uint64_t)edge(i), b = (uint64_t)edge(j), want;
            int want_flag = __builtin_add_overflow(a, b, &want);
            uint8_t flag = 7;
            uint64_t got = k_uadd_i64(a, b, &flag);
            if (got != want || flag != want_flag) {
                if (failures++ < 20)
                    printf("uadd_overflow.i64(%#llx, %#llx) = %#llx flag %d, want %#llx flag %d\n",
                           (unsigned long long)a, (unsigned long long)b, (unsigned long long)got, flag,
                           (unsigned long long)want, want_flag);
            }
        }
}

static void check_sadd_i64(void) {
    for (int i = 0; i < COUNT; i++)
        for (int j = 0; j < COUNT; j++) {
            int64_t a = (int64_t)edge(i), b = (int64_t)edge(j), want;
            int want_flag = __builtin_add_overflow(a, b, &want);
            uint8_t flag = 7;
            int64_t got = k_sadd_i64(a, b, &flag);
            if (got != want || flag != want_flag) {
                if (failures++ < 20)
                    printf("sadd_overflow.i64(%#llx, %#llx) = %#llx flag %d, want %#llx flag %d\n",
                           (unsigned long long)a, (unsigned long long)b, (unsigned long long)got, flag,
                           (unsigned long long)want, want_flag);
            }
        }
}

static void check_usub_i64(void) {
    for (int i = 0; i < COUNT; i++)
        for (int j = 0; j < COUNT; j++) {
            uint64_t a = (uint64_t)edge(i), b = (uint64_t)edge(j), want;
            int want_flag = __builtin_sub_overflow(a, b, &want);
            uint8_t flag = 7;
            uint64_t got = k_usub_i64(a, b, &flag);
            if (got != want || flag != want_flag) {
                if (failures++ < 20)
                    printf("usub_overflow.i64(%#llx, %#llx) = %#llx flag %d, want %#llx flag %d\n",
                           (unsigned long long)a, (unsigned long long)b, (unsigned long long)got, flag,
                           (unsigned long long)want, want_flag);
            }
        }
}

static void check_ssub_i64(void) {
    for (int i = 0; i < COUNT; i++)
        for (int j = 0; j < COUNT; j++) {
            int64_t a = (int64_t)edge(i), b = (int64_t)edge(j), want;
            int want_flag = __builtin_sub_overflow(a, b, &want);
            uint8_t flag = 7;
            int64_t got = k_ssub_i64(a, b, &flag);
            if (got != want || flag != want_flag) {
                if (failures++ < 20)
                    printf("ssub_overflow.i64(%#llx, %#llx) = %#llx flag %d, want %#llx flag %d\n",
                           (unsigned long long)a, (unsigned long long)b, (unsigned long long)got, flag,
                           (unsigned long long)want, want_flag);
            }
        }
}

static void check_umul_i64(void) {
    for (int i = 0; i < COUNT; i++)
        for (int j = 0; j < COUNT; j++) {
            uint64_t a = (uint64_t)edge(i), b = (uint64_t)edge(j), want;
            int want_flag = __builtin_mul_overflow(a, b, &want);
            uint8_t flag = 7;
            uint64_t got = k_umul_i64(a, b, &flag);
            if (got != want || flag != want_flag) {
                if (failures++ < 20)
                    printf("umul_overflow.i64(%#llx, %#llx) = %#llx flag %d, want %#llx flag %d\n",
                           (unsigned long long)a, (unsigned long long)b, (unsigned long long)got, flag,
                           (unsigned long long)want, want_flag);
            }
        }
}

static void check_smul_i64(void) {
    for (int i = 0; i < COUNT; i++)
        for (int j = 0; j < COUNT; j++) {
            int64_t a = (int64_t)edge(i), b = (int64_t)edge(j), want;
            int want_flag = __builtin_mul_overflow(a, b, &want);
            uint8_t flag = 7;
            int64_t got = k_smul_i64(a, b, &flag);
            if (got != want || flag != want_flag) {
                if (failures++ < 20)
                    printf("smul_overflow.i64(%#llx, %#llx) = %#llx flag %d, want %#llx flag %d\n",
                           (unsigned long long)a, (unsigned long long)b, (unsigned long long)got, flag,
                           (unsigned long long)want, want_flag);
            }
        }
}

static void check_uadd_i128(void) {
    for (int i = 0; i < COUNT; i++)
        for (int j = 0; j < COUNT; j++) {
            u128 a = (u128)edge(i), b = (u128)edge(j), want;
            int want_flag = __builtin_add_overflow(a, b, &want);
            uint8_t flag = 7;
            u128 got = k_uadd_i128(a, b, &flag);
            if (got != want || flag != want_flag) {
                if (failures++ < 20)
                    printf("uadd_overflow.i128(%#llx, %#llx) = %#llx flag %d, want %#llx flag %d\n",
                           (unsigned long long)a, (unsigned long long)b, (unsigned long long)got, flag,
                           (unsigned long long)want, want_flag);
            }
        }
}

static void check_sadd_i128(void) {
    for (int i = 0; i < COUNT; i++)
        for (int j = 0; j < COUNT; j++) {
            i128 a = (i128)edge(i), b = (i128)edge(j), want;
            int want_flag = __builtin_add_overflow(a, b, &want);
            uint8_t flag = 7;
            i128 got = k_sadd_i128(a, b, &flag);
            if (got != want || flag != want_flag) {
                if (failures++ < 20)
                    printf("sadd_overflow.i128(%#llx, %#llx) = %#llx flag %d, want %#llx flag %d\n",
                           (unsigned long long)a, (unsigned long long)b, (unsigned long long)got, flag,
                           (unsigned long long)want, want_flag);
            }
        }
}

static void check_usub_i128(void) {
    for (int i = 0; i < COUNT; i++)
        for (int j = 0; j < COUNT; j++) {
            u128 a = (u128)edge(i), b = (u128)edge(j), want;
            int want_flag = __builtin_sub_overflow(a, b, &want);
            uint8_t flag = 7;
            u128 got = k_usub_i128(a, b, &flag);
            if (got != want || flag != want_flag) {
                if (failures++ < 20)
                    printf("usub_overflow.i128(%#llx, %#llx) = %#llx flag %d, want %#llx flag %d\n",
                           (unsigned long long)a, (unsigned long long)b, (unsigned long long)got, flag,
                           (unsigned long long)want, want_flag);
            }
        }
}

static void check_ssub_i128(void) {
    for (int i = 0; i < COUNT; i++)
        for (int j = 0; j < COUNT; j++) {
            i128 a = (i128)edge(i), b = (i128)edge(j), want;
            int want_flag = __builtin_sub_overflow(a, b, &want);
            uint8_t flag = 7;
            i128 got = k_ssub_i128(a, b, &flag);
            if (got != want || flag != want_flag) {
                if (failures++ < 20)
                    printf("ssub_overflow.i128(%#llx, %#llx) = %#llx flag %d, want %#llx flag %d\n",
                           (unsigned long long)a, (unsigned long long)b, (unsigned long long)got, flag,
                           (unsigned long long)want, want_flag);
            }
        }
}

int main(void) {
    check_uadd_i8();
    check_sadd_i8();
    check_usub_i8();
    check_ssub_i8();
    check_umul_i8();
    check_smul_i8();
    check_uadd_i16();
    check_sadd_i16();
    check_usub_i16();
    check_ssub_i16();
    check_umul_i16();
    check_smul_i16();
    check_uadd_i32();
    check_sadd_i32();
    check_usub_i32();
    check_ssub_i32();
    check_umul_i32();
    check_smul_i32();
    check_uadd_i64();
    check_sadd_i64();
    check_usub_i64();
    check_ssub_i64();
    check_umul_i64();
    check_smul_i64();
    check_uadd_i128();
    check_sadd_i128();
    check_usub_i128();
    check_ssub_i128();
    if (k_live(UINT64_MAX, 2) != 2 * 1 + 1 || k_live(5, 6) != 22) {
        printf("k_live wrong\n");
        failures++;
    }
    /* An exit status is taken mod 256. */
    return failures != 0;
}
