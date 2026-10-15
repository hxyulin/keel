#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

extern uint64_t constant_loop(uint64_t n);

int main(int argc, char **argv) {
    uint64_t n = argc > 1 ? strtoull(argv[1], 0, 10) : 10000;
    uint64_t result = constant_loop(n);
    if (result != n * 49) return 1;
    printf("%llu\n", (unsigned long long)result);
    return 0;
}
