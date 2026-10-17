#include <stdint.h>

/* Mathematical closed form; the measured implementation is assembly. */
uint64_t pattern_oracle(uint64_t iterations, uint64_t seed,
                        const uint64_t *data, uint64_t mask) {
    (void)data;
    (void)mask;
    return seed + UINT64_C(1) * iterations;
}
