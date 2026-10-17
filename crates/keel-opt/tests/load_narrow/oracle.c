/* Compile separately with Clang -O0 -fno-lto; never link Keel-generated code
 * into the oracle compilation. memcpy permits unaligned ordinary accesses. */
#include <stdint.h>
#include <string.h>

uint64_t oracle_stack(uint64_t word, unsigned offset) {
    unsigned char object[32];
    uint64_t value;
    memcpy(object + offset, &word, sizeof word);
    memcpy(&value, object + offset, sizeof value);
    return value & UINT64_C(255);
}

uint64_t oracle_write(uint64_t first, uint64_t second) {
    unsigned char object[32];
    uint64_t value;
    memcpy(object + 1, &first, sizeof first);
    memcpy(&value, object + 1, sizeof value);
    memcpy(object + 1, &second, sizeof second);
    return value & UINT64_C(255);
}

uint64_t oracle_generic(const void *address) {
    uint64_t value;
    memcpy(&value, address, sizeof value);
    return value & UINT64_C(255);
}

uint64_t oracle_bridge(uint64_t n, uint64_t state,
                       const uint64_t *data, uint64_t mask) {
    while (n != 0) {
        uint64_t index = (state >> 8) & mask;
        state = data[index] & UINT64_C(255);
        --n;
    }
    return state;
}
