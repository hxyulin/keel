#include <stdint.h>
#include <stdio.h>

unsigned long stack_register_snapshot(unsigned long,
    unsigned long, unsigned long, unsigned long, unsigned long,
    unsigned long, unsigned long, unsigned long, unsigned long *);
int ordinary_register_local(int);

int main(void) {
    const unsigned long a = UINT64_C(0xfedcba9876543210);
    const unsigned long b = UINT64_C(0x123456789abcdef0);
    const unsigned long c = UINT64_C(0x8000000000000001);
    const unsigned long d = UINT64_C(0xffff00000000ffff);
    const unsigned long e = UINT64_C(0x1010101010101010);
    const unsigned long f = UINT64_C(0xeeeeeeeeeeeeeeee);
    const unsigned long g = UINT64_C(0xabcdef1234567890);
    const unsigned long sum = (a ^ (b + 3)) + (c ^ (d + 5)) + (e ^ (f + 7)) + g;
    if (ordinary_register_local(41) != 42) return 1;
    for (unsigned long size = 1; size <= 257; ++size) {
        unsigned long out[8] = { 0, 0, 0, 0, 0, 0,
            UINT64_C(0xa55aa55aa55aa55a), UINT64_C(0x5aa55aa55aa55aa5) };
        unsigned long delta = stack_register_snapshot(size, a, b, c, d, e, f, g, out);
        if (out[0] != out[1] || out[2] != out[3]
            || out[0] <= out[2] || delta != out[0] - out[2]
            || delta < size || delta % 16 || out[4] != (size == 1 ? 58 : 42)
            || out[5] != sum || out[6] != UINT64_C(0xa55aa55aa55aa55a)
            || out[7] != UINT64_C(0x5aa55aa55aa55aa5)) {
            fprintf(stderr, "stack register mismatch at size %lu\n", size);
            return 1;
        }
    }
    puts("stack-register PASS");
    return 0;
}
