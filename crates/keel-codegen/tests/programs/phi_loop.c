#include <stdint.h>
extern uint64_t phi_loop(uint64_t x, uint64_t count);
extern uint64_t edge_transfers(const unsigned char *, uint64_t, uint64_t, uint64_t);
int main(void) {
    uint64_t inputs[] = {0, 1, UINT64_MAX, UINT64_C(1) << 63};
    for (unsigned i = 0; i < 4; ++i)
        for (uint64_t count = 0; count < 100; ++count)
            if (phi_loop(inputs[i], count) != inputs[i]) return 1;
    unsigned char decisions[32];
    for (unsigned pattern = 0; pattern < 4; ++pattern) {
        for (unsigned i = 0; i < 32; ++i)
            decisions[i] = pattern < 2 ? pattern : ((i * 13 + pattern) & 3);
        for (unsigned n = 0; n <= 32; ++n) {
            for (unsigned seed = 0; seed < 4; ++seed) {
                uint64_t a = inputs[seed], b = inputs[3 - seed];
                for (unsigned i = 0; i < n; ++i) {
                    uint64_t old_a = a, old_b = b;
                    a = decisions[i] ? old_b : old_a + old_b;
                    b = decisions[i] ? old_a ^ old_b : old_a;
                }
                if (edge_transfers(decisions, n, inputs[seed], inputs[3 - seed]) != (a ^ b))
                    return 2;
            }
        }
    }
    return 0;
}
