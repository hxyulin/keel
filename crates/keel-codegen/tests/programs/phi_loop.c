#include <stdint.h>
extern uint64_t phi_loop(uint64_t x, uint64_t count);
int main(void) {
    uint64_t inputs[] = {0, 1, UINT64_MAX, UINT64_C(1) << 63};
    for (unsigned i = 0; i < 4; ++i)
        for (uint64_t count = 0; count < 100; ++count)
            if (phi_loop(inputs[i], count) != inputs[i]) return 1;
    return 0;
}
