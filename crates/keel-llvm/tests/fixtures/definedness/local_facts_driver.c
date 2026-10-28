#include <stdint.h>
#include <stdio.h>

extern uint64_t local_range(uint64_t *);
extern uint64_t masked_local_range(uint64_t *);
extern uint64_t local_pointer(void **);
extern uint64_t masked_local_pointer(void **);
extern uint64_t local_nullable(void **);
extern uint64_t separate_loads(uint64_t *);

void overwrite_cell(uint64_t *cell) { *cell = 999; }

int main(void) {
    _Alignas(16) uint64_t data[4] = {42, 0, 0, 0};
    void *cell = data;
    if (local_pointer(&cell) != 42 || local_nullable(&cell) != 42) return 1;
    cell = 0;
    if (masked_local_pointer(&cell) != 1 || local_nullable(&cell) != 0) return 2;
    cell = (char *)data + 1;
    if (masked_local_pointer(&cell) != 1) return 3;
    for (uint64_t i = 0; i < 128; ++i) {
        uint64_t value = i;
        if (local_range(&value) != i + 1 || masked_local_range(&value) != 73) return 4;
        if (separate_loads(&value) != i + 999 || value != 999) return 5;
    }
    puts("local-facts-ok");
    return 0;
}
