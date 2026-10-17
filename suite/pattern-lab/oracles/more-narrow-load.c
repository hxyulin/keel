#include <stdint.h>

/* Independent unsigned mathematical recurrence; all arithmetic wraps mod 2^64. */
/* Ordinary immutable storage only. The byte-load variant assumes little endian.
 * mask=255 and 256 allocated uint64_t words bound every full-width access. */
uint64_t pattern_oracle(uint64_t iterations, uint64_t seed,
                        const uint64_t *data, uint64_t mask) {
    uint64_t state = seed;
    while (iterations != 0) {
        uint64_t index = (state >> 8) & mask;
        state = data[index] & UINT64_C(255);
        iterations--;
    }
    return state;
}
