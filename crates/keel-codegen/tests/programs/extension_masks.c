#include <stdint.h>
extern uint64_t mask_uext32(uint64_t);
extern uint64_t mask_sext32(uint64_t);
extern uint64_t mask_sign32(uint64_t);
extern uint64_t mask_wide8(uint64_t);
extern uint64_t mask_wide_sign(uint64_t);
static int check(uint64_t x) {
    /* Construct the sign-extension oracle entirely with unsigned bitvectors. */
    uint64_t negative = (x & UINT64_C(0x80000000)) ? UINT64_C(0xffffffff80000000) : 0;
    return mask_uext32(x) != (x & UINT64_C(0xffffffff)) ||
           mask_sext32(x) != (x & UINT64_C(0x7fffffff)) ||
           mask_sign32(x) != negative || mask_wide8(x) != (x & 255) ||
           mask_wide_sign(x) != 0;
}
int main(void) {
    const uint64_t edges[] = {0, 1, 127, 128, 255, 256, UINT64_C(0x7fffffff),
        UINT64_C(0x80000000), UINT64_C(0xffffffff), UINT64_C(0x100000000),
        UINT64_C(0x7fffffffffffffff), UINT64_C(0x8000000000000000), UINT64_MAX};
    for (uint64_t x = 0; x < 65536; ++x) if (check(x)) return 1;
    for (unsigned i = 0; i < sizeof edges / sizeof *edges; ++i)
        if (check(edges[i])) return 2;
    return 0;
}
