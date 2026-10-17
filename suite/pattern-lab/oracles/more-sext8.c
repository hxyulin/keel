#include <stdint.h>

/* Independent unsigned mathematical recurrence; all arithmetic wraps mod 2^64. */
uint64_t pattern_oracle(uint64_t iterations, uint64_t seed,
                        const uint64_t *data, uint64_t mask) {
    (void)data;
    (void)mask;
    uint64_t state = seed;
    while (iterations != 0) {
        uint64_t byte = state & UINT64_C(255);
        uint64_t extended = byte < UINT64_C(128) ? byte : byte - UINT64_C(256);
        state = extended + UINT64_C(0x9e3779b97f4a7c15);
        iterations--;
    }
    return state;
}
