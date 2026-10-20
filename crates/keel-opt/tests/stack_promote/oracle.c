#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
extern uint64_t private_acc(const uint64_t *, uint64_t);
static uint64_t oracle(const uint64_t *p, uint64_t n) {
    uint64_t lo = 0, hi = 0;
    uint32_t count = 0;
    for (uint64_t i = 0; i < n; ++i) {
        if (p[i] >> 63) lo += p[i]; else hi += p[i];
        ++count;
    }
    return lo * 3 + hi + count;
}
int main(void) {
    uint64_t p[129], state = 1;
    unsigned checks = 0;
    if (private_acc(NULL, 0) != 0) abort();
    ++checks;
    for (unsigned trial = 0; trial < 80; ++trial) {
        for (unsigned i = 0; i < 129; ++i) {
            state ^= state << 13; state ^= state >> 7; state ^= state << 17;
            static const uint64_t edge[] = {0, 1, UINT64_MAX, UINT64_C(1)<<63,
                (UINT64_C(1)<<63)-1, UINT32_MAX, 255, 256};
            p[i] = trial < 8 ? edge[(i + trial) % 8] : state;
        }
        for (unsigned n = 0; n <= 129; ++n) {
            uint64_t expected = oracle(p, n), got = private_acc(p, n);
            if (got != expected) { fprintf(stderr, "%u/%u: %llu != %llu\n", trial, n,
                (unsigned long long)got, (unsigned long long)expected); abort(); }
            ++checks;
        }
    }
    printf("%u independent native oracle cases passed\n", checks);
}
