#include <stdint.h>
#include <stdio.h>

unsigned long goto_pressure(int, unsigned long, unsigned long, unsigned long,
                            unsigned long, unsigned long, unsigned long,
                            unsigned long, unsigned long);
int goto_numeric(int, int);
int goto_same_target(int);
int goto_backwards(int);
int goto_evaluate_once(int);
int goto_inlined(int);
int goto_metadata(int);
int goto_relative_metadata(int);
int goto_memory(int *, int);

static int check(const char *name, unsigned long got, unsigned long want, int x) {
    if (got == want) return 0;
    fprintf(stderr, "%s(%d): got %lu, expected %lu\n", name, x, got, want);
    return 1;
}

int main(void) {
    const unsigned long a = UINT64_C(0xfedcba9876543210);
    const unsigned long b = UINT64_C(0x123456789abcdef0);
    const unsigned long c = UINT64_C(0x8000000000000001);
    const unsigned long d = UINT64_C(0xffff00000000ffff);
    const unsigned long e = UINT64_C(0x1010101010101010);
    const unsigned long f = UINT64_C(0xeeeeeeeeeeeeeeee);
    const unsigned long g = UINT64_C(0x7fffffffffffffff);
    const unsigned long h = UINT64_C(0xabcdef1234567890);
    const unsigned long sum = (a ^ (b + 3)) + (c ^ (d + 5)) + (e ^ (f + 7)) + (g ^ (h + 11));
    for (int x = -128; x <= 128; ++x) {
        if (check("pressure", goto_pressure(x, a, b, c, d, e, f, g, h), sum + (x < 0 ? 17 : x ? 23 : 11), x)) return 1;
        for (int y = -3; y <= 3; ++y) {
            if (check("numeric", goto_numeric(x, y), x < y ? 13 : x > y ? 19 : 5, x)) return 1;
        }
        if (check("same", goto_same_target(x), x ? 31 : 29, x)) return 1;
        if (check("once", goto_evaluate_once(x), x + 1 + (x == 2 ? 20 : 10), x)) return 1;
        if (check("inlined", goto_inlined(x), (x ? 7 : 3) * 11 + (x + 1 ? 7 : 3), x)) return 1;
        if (check("absolute metadata", goto_metadata(x), x ? 41 : 37, x)) return 1;
        if (check("relative metadata", goto_relative_metadata(x), x ? 47 : 43, x)) return 1;
        int object = x;
        if (check("memory", goto_memory(&object, x), x + 17 + (x ? 59 : 53), x)) return 1;
        if (object != 17) return 1;
    }
    for (int n = 0; n < 300; ++n) {
        if (check("backwards", goto_backwards(n), n > 0 ? n : 1, n)) return 1;
    }
    puts("asm-goto PASS");
    return 0;
}
