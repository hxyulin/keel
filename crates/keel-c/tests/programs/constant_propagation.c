#include <stdint.h>
#include <stdio.h>

static uint64_t loop(uint64_t limit, int choose) {
    uint64_t invariant;
    if (choose) invariant = 7;
    else invariant = 7;
    uint64_t result = 0;
    for (uint64_t i = 0; i < limit; ++i)
        result += invariant * invariant + i;
    return result;
}

static unsigned branch(unsigned input) {
    unsigned value;
    if (input & 1) value = 40;
    else value = 99;
    return value + 2;
}

static unsigned changing_loop(void) {
    unsigned i = 0;
    do { ++i; } while (i < 3);
    return i;
}

static int constant_branch(int input) {
    int value;
    if (input) value = 1;
    else value = 1;
    if (value) return 42;
    return 99;
}

int main(void) {
    uint64_t checksum = 0;
    for (unsigned i = 0; i < 128; ++i) {
        checksum += loop(i, i & 1);
        checksum += branch(i);
        checksum += constant_branch(i);
    }
    printf("%llu %u\n", (unsigned long long)checksum, changing_loop());
    return 0;
}
