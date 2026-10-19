/* Driver for bridge.keel. `bridge` prints all results; `bridge trap NAME`
 * performs one trapping operation and must terminate abnormally. */
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

typedef uint64_t u64;
typedef int64_t i64;
extern u64 sdiv64(u64, u64), srem64(u64, u64), udiv64(u64, u64), urem64(u64, u64);
extern u64 sdiv8(u64, u64), srem8(u64, u64), udiv16(u64, u64), srem32(u64, u64);
extern u64 shift_mixed(u64, u64), cmp_narrow(u64, u64), mulhi(u64, u64);
extern u64 switch_args(u64), switch_i8(u64), brif_same(u64, u64), entry_loop(u64, u64);
extern u64 indirect(u64), call_c_narrow(u64), fconv_narrow(u64), do_trap(void);
extern short narrow_abi(signed char, unsigned short);
extern void vec_int(void *, void *, void *), vec_float(void *, void *, void *);
extern void vec_select(void *, void *, u64);
extern void vec_i128(void *);

/* Relies on Apple's caller-extension rule: arguments arrive extended. */
signed char c_narrow(signed char a, unsigned short b) { return (signed char)(a * 7 - (int)b); }

static void show(u64 x) { printf("%016llx\n", (unsigned long long)x); }
static void show_bytes(const void *p, unsigned n) {
    const unsigned char *b = p;
    for (unsigned i = 0; i < n; ++i) printf("%02x", b[i]);
    printf("\n");
}

int main(int argc, char **argv) {
    volatile u64 zero = 0;
    if (argc == 3 && !strcmp(argv[1], "trap")) {
        const char *t = argv[2];
        u64 r = 0;
        if (!strcmp(t, "sdiv0")) r = sdiv64(5, zero);
        else if (!strcmp(t, "udiv0")) r = udiv64(5, zero);
        else if (!strcmp(t, "srem0")) r = srem64(5, zero);
        else if (!strcmp(t, "urem0")) r = urem64(5, zero);
        else if (!strcmp(t, "sdivmin")) r = sdiv64(UINT64_C(0x8000000000000000), (u64)-1 + zero);
        else if (!strcmp(t, "sdiv8min")) r = sdiv8(0x80, 0xff + zero);
        else if (!strcmp(t, "trap")) r = do_trap();
        printf("survived %016llx\n", (unsigned long long)r);
        return 0;
    }
    u64 v[] = {0, 1, 2, 3, 7, 8, 15, 16, 31, 32, 63, 64, 65, 127, 128, 129, 255, 256, 1000000,
               UINT64_C(0x7f), UINT64_C(0x80), UINT64_C(0x7fff), UINT64_C(0x8000), UINT64_C(0x7fffffff),
               UINT64_C(0x80000000), UINT64_C(0xffffffff), UINT64_C(0x7fffffffffffffff),
               UINT64_C(0x8000000000000000), (u64)-1, (u64)-2, (u64)-5, (u64)-128, (u64)-129,
               UINT64_C(0x123456789abcdef0), UINT64_C(0xfedcba9876543210)};
    unsigned n = sizeof v / sizeof *v;
    for (unsigned i = 0; i < n; ++i) {
        u64 x = v[i];
        show(switch_args(x)); show(switch_i8(x)); show(indirect(x));
        show(call_c_narrow(x)); show(fconv_narrow(x));
        show((u64)(i64)narrow_abi((signed char)x, (unsigned short)(x >> 3)));
        show(entry_loop(x & 63, x));
        for (unsigned j = 0; j < n; ++j) {
            u64 y = v[j];
            show(shift_mixed(x, y)); show(cmp_narrow(x, y)); show(mulhi(x, y)); show(brif_same(x, y));
            if (y) {
                show(udiv64(x, y)); show(urem64(x, y)); show(srem64(x, y));
                if (!(x == UINT64_C(0x8000000000000000) && y == (u64)-1)) show(sdiv64(x, y));
            }
            if ((uint8_t)y) {
                show(srem8(x, y));
                if (!((uint8_t)x == 0x80 && (uint8_t)y == 0xff)) show(sdiv8(x, y));
            }
            if ((uint16_t)y) show(udiv16(x, y));
            if ((uint32_t)y && !((uint32_t)x == 0x80000000u && (uint32_t)y == 0xffffffffu)) show(srem32(x, y));
        }
    }
    /* srem MIN % -1 is defined as 0 at every width. */
    show(srem64(UINT64_C(0x8000000000000000), (u64)-1));
    show(srem8(0x80, 0xff));
    int32_t ia[4] = {5, -7, 2147483647, -2147483647 - 1}, ib[4] = {3, 33, -1, 100};
    unsigned char out[32];
    for (int round = 0; round < 4; ++round) {
        vec_int(ia, ib, out);
        show_bytes(out, 16);
        for (int k = 0; k < 4; ++k) { ia[k] = ia[k] * 1103515245 + 12345 + round; ib[k] ^= ia[k] >> 3; }
    }
    float fa[8] = {1.5f, -0.0f, NAN, 4.0f, 0.0f, -2.5f, INFINITY, 9.0f};
    float fb[8] = {2.0f, 0.0f, 1.0f, -NAN, -0.0f, 2.5f, 3.0f, -INFINITY};
    vec_float(fa, fb, out);
    show_bytes(out, 32);
    vec_float(fb, fa, out);
    show_bytes(out, 32);
    int64_t sa[2] = {11, -12}, sb[2] = {100, 200};
    vec_select(sa, sb, 1);
    show_bytes(sa, 16);
    vec_select(sa, sb, 0);
    show_bytes(sa, 16);
    uint32_t wide[4] = {0x80000001u, 0xffffffffu, 0x12345678u, 0x7fffffffu};
    for (int k = 0; k < 3; ++k) {
        vec_i128(wide);
        show_bytes(wide, 16);
    }
    return 0;
}
