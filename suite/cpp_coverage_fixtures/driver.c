#include <stdio.h>
typedef unsigned long long U;
extern U through_c(U, U, volatile U *);
static U random_value(U *s) { *s ^= *s << 13; *s ^= *s >> 7; *s ^= *s << 17; return *s; }
int main(void) {
    const U edges[] = {0, 1, 2, 3, 7, 255, 256, 65535, 65536,
        0x7fffffffffffffffULL, 0x8000000000000000ULL, ~0ULL};
    U state = 0x123456789abcdefULL, sum = 0;
    unsigned count = 0;
    for (unsigned i = 0; i < 4240; ++i) {
        U a = i < 144 ? edges[i / 12] : random_value(&state);
        U b = i < 144 ? edges[i % 12] : random_value(&state);
        volatile U cells[2] = {a ^ 31, b ^ 67};
        U result = through_c(a, b, cells);
        /* Emit every result and both memory cells: no checksum-only oracle. */
        printf("%016llx %016llx %016llx\n", result, cells[0], cells[1]);
        sum ^= result + cells[0] + cells[1];
        ++count;
    }
    printf("cases=%u checksum=%016llx\n", count, sum);
    return 0;
}
