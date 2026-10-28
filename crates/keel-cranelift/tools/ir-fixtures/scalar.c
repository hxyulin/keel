#include <stdint.h>
#include <stdio.h>
extern uint64_t probe(uint64_t, uint64_t);
int main(void) {
    const uint64_t edge[] = {0, 1, 2, 63, 64, 65, INT64_MAX, UINT64_C(1) << 63, UINT64_MAX};
    uint64_t state = UINT64_C(0x78e24ca31);
    for (unsigned i = 0; i < 10081; ++i) {
        uint64_t a, b;
        if (i < 81) { a = edge[i / 9]; b = edge[i % 9]; }
        else {
            state ^= state << 13; state ^= state >> 7; state ^= state << 17; a = state;
            state ^= state << 13; state ^= state >> 7; state ^= state << 17; b = state;
        }
        uint64_t expected = a < b ? (a + b) ^ (a << (b & 63)) : a - b;
        uint64_t got = probe(a, b);
        if (got != expected) {
            fprintf(stderr, "input %u: %llu, %llu: got %llu, expected %llu\n", i,
                    (unsigned long long)a, (unsigned long long)b,
                    (unsigned long long)got, (unsigned long long)expected);
            return 1;
        }
    }
    puts("ok");
    return 0;
}
