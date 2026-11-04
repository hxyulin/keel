// Integer constant expressions at every width, as static initializers and
// at runtime through volatile operands. Each line is one failure mode of
// constant evaluation: 64-bit wrapping of 128-bit arithmetic, widening of
// unsigned 64-bit values, the stored extension of unsigned 128-bit values,
// and conversions between 128-bit integers and floating types.
#include <limits.h>
#include <stdio.h>

typedef __int128 i128;
typedef unsigned __int128 u128;

static void hex(const char *name, u128 v) {
    printf("%s %016llx%016llx\n", name, (unsigned long long)(v >> 64), (unsigned long long)v);
}

#define BOTH(name, type, expr, runtime) \
    static const type s_##name = (expr); \
    static void name(void) { hex("static " #name, (u128)s_##name); hex("runtime " #name, (u128)(type)(runtime)); }

static volatile int vi = 1;
static volatile unsigned vu = 1;
static volatile long vl = 1;
static volatile unsigned long vul = 1;
static volatile i128 vs = 1;
static volatile u128 vw = 1;

BOTH(int_wrap, unsigned, INT_MAX + 1u, INT_MAX + vu)
BOTH(int_div, int, -7 / 2 + -7 % 2 * 100, -7 / (2 * vi) + -7 % (2 * vi) * 100)
BOTH(int_shr, int, (-8 >> 1) + (0x7fffffff >> 30), (-8 * vi >> 1) + (0x7fffffff * vi >> 30))
BOTH(int_min, int, (int)(1u << 31), (int)(vu << 31))
BOTH(uint_shr, unsigned, (unsigned)-1 >> 1, (unsigned)-vi >> 1)
BOTH(long_div, long, LONG_MIN / 3, LONG_MIN / (3 * vl))
BOTH(long_cmp, int, (-1L < 0L) + (-1L < 0UL) * 2, (-vl < 0L) + (-vl < 0UL) * 2)
BOTH(ulong_mul, unsigned long, ULONG_MAX * ULONG_MAX, ULONG_MAX * (ULONG_MAX * vul))
BOTH(ulong_shift, unsigned long, (1UL << 63) >> 62, (vul << 63) >> 62)

BOTH(shl70, u128, (u128)1 << 70, vw << 70)
BOTH(div_above64, u128, ((u128)1 << 64) / 3, (vw << 64) / 3)
BOTH(shl63, i128, (i128)1 << 63, vs << 63)
BOTH(shr64, u128, ~(u128)0 >> 64, ~(vw - 1) >> 64)
BOTH(signed_shr, i128, (i128)((u128)-1 << 100) >> 70, (i128)(-vw << 100) >> 70)
BOTH(widen_ulong, u128, 0xffffffffffffffffull, 0xffffffffffffffffull * vw)
BOTH(mixed, u128, 0xffffffffffffffffull * (u128)0xffffffffffffffffull, 0xffffffffffffffffull * (0xffffffffffffffffull * vw))
BOTH(neg_one, u128, -1, -vw)
BOTH(i128_min, i128, -(i128)((u128)1 << 126) * 2, -(i128)(vw << 126) * 2)
BOTH(i128_div, i128, (-((i128)1 << 100) - 5) / 7 + (-((i128)1 << 100) - 5) % 7, (-(vs << 100) - 5) / 7 + (-(vs << 100) - 5) % 7)
BOTH(i128_cmp, int, ((i128)-1 < (i128)1 << 64) + ((u128)-1 > (u128)1 << 127) * 2, (-vs < vs << 64) + (-vw > vw << 127) * 2)
BOTH(narrow, unsigned long, (unsigned long)(((u128)5 << 64) | 7), (unsigned long)((vw * 5 << 64) | 7))

// Constant conversions between 128-bit integers and floating types. Keel
// does not yet convert these at runtime.
static const double d_umax = (double)(u128)-1;
static const float f_umax = (float)(u128)-1;
static const double d_min = (double)(i128)((u128)1 << 127);
static const double d_100 = (double)((i128)1 << 100);
static const u128 from_1e30 = (u128)1e30;
static const i128 from_neg = (i128)-1e30;
static const u128 from_2_127 = (u128)0x1p127;

int main(void) {
    int_wrap();
    int_div();
    int_shr();
    int_min();
    uint_shr();
    long_div();
    long_cmp();
    ulong_mul();
    ulong_shift();
    shl70();
    div_above64();
    shl63();
    shr64();
    signed_shr();
    widen_ulong();
    mixed();
    neg_one();
    i128_min();
    i128_div();
    i128_cmp();
    narrow();
    printf("%a %a %a %a\n", d_umax, (double)f_umax, d_min, d_100);
    hex("from_1e30", from_1e30);
    hex("from_neg", (u128)from_neg);
    hex("from_2_127", from_2_127);
    return 0;
}
