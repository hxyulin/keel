#include <stdint.h>
#include <stdio.h>
#include <inttypes.h>
extern uint64_t large_chain(uint64_t, uint64_t, uint64_t, uint64_t *);
extern uint64_t large_join(uint64_t, uint64_t, uint64_t, uint64_t *);
extern uint64_t large_loop(uint64_t, uint64_t, uint64_t, uint64_t *);
typedef uint64_t (*kernel)(uint64_t, uint64_t, uint64_t, uint64_t *);
static int check(uint64_t n, uint64_t seed, uint64_t cond) {
    const kernel functions[] = {large_chain, large_join, large_loop};
    const uint64_t steps[] = {1, 3, 7, 17};
    const uint64_t masks[] = {UINT8_MAX, UINT16_MAX, UINT32_MAX, UINT64_MAX};
    for (unsigned k = 0; k < 3; ++k) {
        uint64_t output[5] = {0xdead, 0xbeef, 0x1234, 0x5678, UINT64_C(0x123456789abcdef0)};
        uint64_t base = seed + (k == 1 && !cond ? 123 : 0), sum = 0;
        uint64_t result = functions[k](n, seed, cond, output);
        for (unsigned field = 0; field < 4; ++field) {
            uint64_t expected = (base + (k == 2 ? n * steps[field] : 0)) & masks[field];
            if (output[field] != expected) {
                fprintf(stderr, "kernel %u field %u n=%"PRIu64" seed=%"PRIu64" cond=%"PRIu64": got=%"PRIu64" expected=%"PRIu64"\n", k, field, n, seed, cond, output[field], expected);
                return 1;
            }
            sum += expected;
        }
        if (result != sum || output[4] != UINT64_C(0x123456789abcdef0)) return 2;
    }
    return 0;
}
int main(void) {
    const uint64_t lengths[] = {0, 1, 2, 7, 31, 127};
    const uint64_t seeds[] = {0,1,127,128,255,256,32767,32768,65535,65536,UINT32_MAX,UINT64_C(0x100000000),INT64_MAX,UINT64_C(0x8000000000000000),UINT64_MAX-1,UINT64_MAX};
    for (unsigned n=0; n<6; ++n)
        for (unsigned s=0; s<16; ++s)
            for (unsigned c=0; c<2; ++c)
                if (check(lengths[n], seeds[s], c)) return 1;
    uint64_t state = UINT64_C(0x75a1b92c3d4e5f60);
    for (unsigned i=0; i<256; ++i) {
        state ^= state << 13; state ^= state >> 7; state ^= state << 17;
        if (check((state >> 32) & 127, state, state & 1)) return 1;
    }
    puts("1344 by-field large-CFG oracle cases passed");
    return 0;
}
