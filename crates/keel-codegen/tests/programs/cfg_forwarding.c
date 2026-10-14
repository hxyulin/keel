#include <stdint.h>
extern uint64_t forward(uint64_t, uint64_t, unsigned char);
int main(void) {
    for (uint64_t a = 0; a < 32; ++a)
        for (uint64_t b = 0; b < 32; ++b) {
            if (forward(a, b, 1) != b * b + a) return 1;
            if (forward(a, b, 0) != a * a + a) return 2;
        }
    return 0;
}
