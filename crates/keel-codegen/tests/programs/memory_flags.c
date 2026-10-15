#include <stdint.h>
extern uint64_t memory_flags(uint64_t *p);
int main(void) {
    uint64_t value = 0;
    for (unsigned i = 0; i < 100; ++i)
        if (memory_flags(&value) != i + 1 || value != i + 1) return 1;
    return 0;
}
