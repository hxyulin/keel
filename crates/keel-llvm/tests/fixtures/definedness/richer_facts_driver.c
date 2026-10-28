#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

extern uint64_t bounded(uint64_t);
extern uint64_t indirect_bounded(uint64_t (*)(uint64_t), uint64_t);
extern uint64_t masked_range(uint64_t);
extern void *returned_copy(void *);
extern uint64_t nullable(void *);
extern uint64_t allocation(uint64_t);

static void *last_allocation;
void *fresh(uint64_t bytes) {
    last_allocation = malloc(bytes);
    if (!last_allocation) abort();
    return last_allocation;
}

int main(void) {
    uint64_t word = UINT64_C(0x8000000000000042);
    if (returned_copy(&word) != &word || returned_copy(0) != 0) return 1;
    if (nullable(&word) != word || nullable(0) != 0) return 2;
    for (uint64_t i = 0; i < 128; ++i) {
        if (bounded(i) != i + 1 || indirect_bounded(bounded, i) != i + 1) return 3;
        /* Range violations produce poison, which this function masks. */
        if (masked_range(i) != 73) return 4;
        if (allocation(i) != i) return 5;
        free(last_allocation);
        last_allocation = 0;
    }
    puts("richer-facts-ok");
    return 0;
}
