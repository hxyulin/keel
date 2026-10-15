#include <stdint.h>
extern uint64_t memory_forward(uint64_t *, uint64_t *, uint64_t);
int main(void) {
    uint64_t edge[] = {0, 1, 255, UINT64_MAX, UINT64_C(1) << 63};
    for (unsigned i = 0; i < 5; ++i) {
        for (unsigned j = 0; j < 5; ++j) {
            uint64_t p = edge[i], q = 7, x = edge[j];
            if (memory_forward(&p, &q, x) != 4 * edge[i]) return 1;
            if (p != edge[i] || q != x) return 2;
            p = edge[i];
            if (memory_forward(&p, &p, x) != 2 * edge[i] + 2 * x) return 3;
            if (p != x) return 4;
        }
    }
    return 0;
}
