#include <stdint.h>

/* Independent unsigned mathematical recurrence; all arithmetic wraps mod 2^64. */
uint64_t pattern_oracle(uint64_t iterations, uint64_t seed,
                        const uint64_t *data, uint64_t mask) {
    uint64_t state = seed;
    while (iterations != 0) {
        uint64_t index = (state >> 8) & mask;
        uint64_t bit = data[index] & UINT64_C(1);
        state = state + (bit ? UINT64_C(3) : UINT64_C(5));
        iterations--;
    }
    return state;
}
