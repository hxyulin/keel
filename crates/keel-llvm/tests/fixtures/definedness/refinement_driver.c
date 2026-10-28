#include <stdint.h>
#include <stdio.h>
#include <string.h>

extern uint64_t masked(_Bool);
extern uint64_t masked_phi(_Bool);
extern uint64_t identity(uint64_t);
extern double fp_identity(double);
extern void *ptr_identity(void *);
extern uint64_t stable_poison(void);
extern uint64_t stable_undef(void);
extern uint64_t undef_range(void);
extern uint64_t flagged(uint64_t);
extern uint64_t flagged_stable(void);
extern uint64_t loop_freeze(uint64_t);

int main(void) {
    const uint64_t bits[] = {0, 1, UINT64_MAX, UINT64_C(0x8000000000000000),
                            UINT64_C(0x7ff8000000000042), UINT64_C(0xfff0000000000000)};
    int object = 0;
    if (masked(0) != 7 || masked_phi(0) != 9) return 1;
    for (unsigned i = 0; i < sizeof(bits) / sizeof(bits[0]); ++i) {
        if (identity(bits[i]) != bits[i]) return 2;
        double input, result;
        uint64_t output;
        memcpy(&input, &bits[i], sizeof(input));
        result = fp_identity(input);
        memcpy(&output, &result, sizeof(output));
        if (output != bits[i]) return 3;
        if (flagged(bits[i]) != bits[i] + 1) return 4;
    }
    if (ptr_identity(&object) != &object || ptr_identity(0) != 0) return 5;
    for (unsigned i = 0; i < 256; ++i) {
        if (stable_poison() != 0 || stable_undef() != 0 || flagged_stable() != 0) return 6;
        /* Original LLVM may choose any value in this range. Do not compare its
           exact choice with the importer's deterministic refinement. */
        if (undef_range() > 255 || loop_freeze(i + 1) != 0) return 7;
    }
    puts("refinement-ok");
    return 0;
}
