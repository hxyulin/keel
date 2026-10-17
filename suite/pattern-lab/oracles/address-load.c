#include <stdint.h>

/* Valid storage: data has at least 512 uint64_t words. The extracted
 * byte bounds the index even if a caller passes a wider mask. */
uint64_t pattern_oracle(uint64_t iterations, uint64_t seed,
                        const uint64_t *data, uint64_t mask) {
    uint64_t state = seed;
    while (iterations != 0) {
        uint64_t index = ((state >> 16) & UINT64_C(255)) & mask;
        state = data[UINT64_C(2) * index];
        --iterations;
    }
    return state;
}
