/* Header-free on both Keel LP64 targets. No OS declarations or signed casts.
 * The ABI uses unsigned long long (64 bits on both supported targets).
 * Memory cases require 256 ordinary immutable words and mask <= 255.
 * At iterations == 0 data may be null; it is never accessed.
 */
typedef unsigned long long uint64_t;
typedef char bridge_requires_64_bits[(sizeof(uint64_t) == 8) ? 1 : -1];

uint64_t bridge_madd(uint64_t iterations, uint64_t seed,
                        const uint64_t *data, uint64_t mask) {
    (void)data;
    (void)mask;
    uint64_t state = seed;
    while (iterations != 0) {
        state = state * 3ULL + 7ULL;
        iterations--;
    }
    return state;
}

uint64_t bridge_rotate(uint64_t iterations, uint64_t seed,
                        const uint64_t *data, uint64_t mask) {
    (void)data;
    (void)mask;
    uint64_t state = seed;
    while (iterations != 0) {
        state = ((state << 51) | (state >> 13)) ^ 0x9e3779b97f4a7c15ULL;
        iterations--;
    }
    return state;
}

uint64_t bridge_udiv10(uint64_t iterations, uint64_t seed,
                        const uint64_t *data, uint64_t mask) {
    (void)data;
    (void)mask;
    uint64_t state = seed;
    while (iterations != 0) {
        state = state / 10ULL + 0x9e3779b97f4a7c15ULL;
        iterations--;
    }
    return state;
}

uint64_t bridge_branch_select(uint64_t iterations, uint64_t seed,
                        const uint64_t *data, uint64_t mask) {
    uint64_t state = seed;
    while (iterations != 0) {
        uint64_t index = (state >> 8) & mask;
        uint64_t bit = data[index] & 1ULL;
        state = state + (bit ? 3ULL : 5ULL);
        iterations--;
    }
    return state;
}

uint64_t bridge_sext8(uint64_t iterations, uint64_t seed,
                        const uint64_t *data, uint64_t mask) {
    (void)data;
    (void)mask;
    uint64_t state = seed;
    while (iterations != 0) {
        uint64_t byte = state & 255ULL;
        uint64_t extended = byte < 128ULL ? byte : byte - 256ULL;
        state = extended + 0x9e3779b97f4a7c15ULL;
        iterations--;
    }
    return state;
}

uint64_t bridge_narrow_load(uint64_t iterations, uint64_t seed,
                        const uint64_t *data, uint64_t mask) {
    uint64_t state = seed;
    while (iterations != 0) {
        uint64_t index = (state >> 8) & mask;
        state = data[index] & 255ULL;
        iterations--;
    }
    return state;
}

