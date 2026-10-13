#include <stdint.h>
#include <stdio.h>

#include "clobber.h"

typedef __int128 i128;
typedef unsigned __int128 u128;

i128 k_iadd(i128, i128);
i128 k_isub(i128, i128);
i128 k_imul(i128, i128);
i128 k_udiv(i128, i128);
i128 k_sdiv(i128, i128);
i128 k_urem(i128, i128);
i128 k_srem(i128, i128);
i128 k_and(i128, i128);
i128 k_or(i128, i128);
i128 k_xor(i128, i128);
i128 k_shl(i128, i128);
i128 k_ushr(i128, i128);
i128 k_sshr(i128, i128);
i128 k_rotl(i128, i128);
i128 k_rotr(i128, i128);
i128 k_ineg(i128);
i128 k_bnot(i128);
i128 k_clz(i128);
i128 k_ctz(i128);
i128 k_popcnt(i128);
i128 k_bswap(i128);
int32_t k_eq(i128, i128);
int32_t k_ne(i128, i128);
int32_t k_slt(i128, i128);
int32_t k_sle(i128, i128);
int32_t k_sgt(i128, i128);
int32_t k_sge(i128, i128);
int32_t k_ult(i128, i128);
int32_t k_ule(i128, i128);
int32_t k_ugt(i128, i128);
int32_t k_uge(i128, i128);
i128 k_select(i128, i128, i128);
i128 k_select32(int32_t, i128, i128);
int64_t k_truth(i128);
i128 k_ext(int64_t);
int64_t k_lo(i128);
int64_t k_hi(i128);
i128 k_mem(i128 *, i128 *);
i128 k_minus_one(void);
i128 k_args(int64_t, i128, int64_t);
i128 k_tail(int64_t, int64_t, int64_t, int64_t, int64_t, int64_t, int64_t, i128, int64_t);
i128 k_call_c(i128);
i128 k_pow(i128, int64_t);
i128 k_tail16(int64_t, int64_t, int64_t, int64_t, int64_t, int64_t, int64_t, int64_t, int64_t,
              i128);
i128 k_rem_live(i128, i128, int64_t);

/* Replaces compiler-rt's, which may happen to leave x9-x15 alone, with one
 * that clobbers them as any call may. Long division, since `%` on u128 would
 * call this. */
u128 __umodti3(u128 a, u128 b) {
    clobber();
    u128 r = 0;
    for (int i = 127; i >= 0; i--) {
        int carry = r >> 127;
        r = r << 1 | (a >> i & 1);
        if (carry || r >= b) r -= b;
    }
    return r;
}

i128 c_args(int64_t a, i128 b, int64_t c) { return a + b * 3 - c; }

i128 c_tail(int64_t a0, int64_t a1, int64_t a2, int64_t a3, int64_t a4, int64_t a5, int64_t a6,
            i128 w, int64_t z) {
    return (i128)(a0 + a1 + a2 + a3 + a4 + a5 + a6) + w * 1000 + (i128)z * 7;
}

static int failures;

#define CHECK(got, want)                                                          \
    do {                                                                          \
        u128 got_ = (u128)(got), want_ = (u128)(want);                            \
        if (got_ != want_) {                                                      \
            printf("line %d: %s = %016llx%016llx, want %016llx%016llx\n", __LINE__, \
                   #got, (unsigned long long)(got_ >> 64), (unsigned long long)got_, \
                   (unsigned long long)(want_ >> 64), (unsigned long long)want_);  \
            failures++;                                                           \
        }                                                                         \
    } while (0)

#define I128(hi, lo) ((i128)(((u128)(uint64_t)(hi) << 64) | (uint64_t)(lo)))

static int clz128(u128 x) {
    int n = 0;
    while (n < 128 && !(x >> (127 - n) & 1)) n++;
    return n;
}
static int ctz128(u128 x) {
    int n = 0;
    while (n < 128 && !(x >> n & 1)) n++;
    return n;
}
static int popcnt128(u128 x) { int n = 0; for (; x; x >>= 1) n += x & 1; return n; }
static u128 bswap128(u128 x) {
    u128 r = 0;
    for (int i = 0; i < 16; i++) r = r << 8 | (uint8_t)(x >> (8 * i));
    return r;
}
static u128 rotl128(u128 x, int n) { n &= 127; return n ? x << n | x >> (128 - n) : x; }

int main(void) {
    const i128 min = (i128)((u128)1 << 127);
    i128 xs[] = {0, 1, -1, 3, -7, 64, 127, 128, I128(0, -1), I128(1, 0), I128(-1, 0), min, ~min,
                 I128(0x0123456789abcdef, 0xfedcba9876543210), -(i128)12345678901234567890u,
                 I128(0x8000000000000000, 1), I128(0x7fffffff, 0x80000000)};
    int n = sizeof xs / sizeof xs[0];
    for (int i = 0; i < n; i++) {
        i128 a = xs[i];
        u128 u = (u128)a;
        CHECK(k_ineg(a), -u);
        CHECK(k_bnot(a), ~a);
        CHECK(k_clz(a), clz128(u));
        CHECK(k_ctz(a), ctz128(u));
        CHECK(k_popcnt(a), popcnt128(u));
        CHECK(k_bswap(a), bswap128(u));
        CHECK(k_truth(a), (a ? 1 : 0) + (a ? 2 : 4));
        CHECK(k_lo(a), (int64_t)a);
        CHECK(k_hi(a), (int64_t)(u >> 64));
        CHECK(k_ext((int64_t)a), (i128)(int64_t)a + 3 * (i128)(uint64_t)a + 5 * (i128)(int32_t)a +
                                     7 * (i128)(uint32_t)a + 11 * (i128)(int8_t)a +
                                     13 * (i128)(uint8_t)a);
        CHECK(k_args((int64_t)a, a, 9), (int64_t)a + (u128)a * 3 - 9);
        CHECK(k_tail(1, 2, 3, 4, 5, 6, (int64_t)a, a, -11),
              (i128)(21 + (int64_t)a) + (u128)a * 1000 - 77);
        CHECK(k_call_c(a), c_args(1, a, 2) + c_tail(1, 2, 3, 4, 5, 6, 7, a, -5) + (u128)a);
        CHECK(k_tail16(1, 2, 3, 4, 5, 6, 7, 8, (int64_t)a, a),
              (i128)(int64_t)(36 + 3 * (uint64_t)a) + (u128)a * 1000);
        CHECK(k_pow(a, i), ({ u128 p = 1; for (int j = 0; j < i; j++) p *= u; p; }));
        for (int s = 0; s < 128; s++) {
            CHECK(k_shl(a, s), u << s);
            CHECK(k_ushr(a, s), u >> s);
            CHECK(k_sshr(a, s), a >> s);
            CHECK(k_rotl(a, s), rotl128(u, s));
            CHECK(k_rotr(a, s), rotl128(u, 128 - s));
        }
        for (int j = 0; j < n; j++) {
            i128 b = xs[j];
            u128 v = (u128)b;
            CHECK(k_iadd(a, b), u + v);
            CHECK(k_isub(a, b), u - v);
            CHECK(k_imul(a, b), u * v);
            CHECK(k_and(a, b), a & b);
            CHECK(k_or(a, b), a | b);
            CHECK(k_xor(a, b), a ^ b);
            /* Shift amounts are taken mod 128. */
            CHECK(k_shl(a, b), u << (v & 127));
            CHECK(k_sshr(a, b), a >> (v & 127));
            CHECK(k_rotr(a, b), rotl128(u, 128 - (int)(v & 127)));
            if (b != 0) {
                CHECK(k_udiv(a, b), u / v);
                CHECK(k_urem(a, b), u % v);
                CHECK(k_rem_live(a, b, (int64_t)a), u % v + u + v + (i128)(int64_t)a);
                if (!(a == min && b == -1)) {
                    CHECK(k_sdiv(a, b), a / b);
                    CHECK(k_srem(a, b), a % b);
                }
            }
            CHECK(k_eq(a, b), a == b);
            CHECK(k_ne(a, b), a != b);
            CHECK(k_slt(a, b), a < b);
            CHECK(k_sle(a, b), a <= b);
            CHECK(k_sgt(a, b), a > b);
            CHECK(k_sge(a, b), a >= b);
            CHECK(k_ult(a, b), (u128)a < (u128)b);
            CHECK(k_ule(a, b), (u128)a <= (u128)b);
            CHECK(k_ugt(a, b), (u128)a > (u128)b);
            CHECK(k_uge(a, b), (u128)a >= (u128)b);
            CHECK(k_select(a, b, ~b), a ? b : ~b);
            CHECK(k_select32((int32_t)a, b, a), (int32_t)a ? b : a);
            i128 p = a, q = b;
            CHECK(k_mem(&p, &q), a);
            CHECK(p, (u128)b + 1);
        }
    }
    CHECK(k_minus_one(), -1);
    /* An exit status is taken mod 256. */
    return failures != 0;
}
