#include <stdint.h>
extern uint64_t strength(uint64_t);
int main(void) {
    for (uint64_t n = 0; n < 1024; ++n) if (strength(n) != n) return 1;
    uint64_t edge[] = {UINT64_MAX, UINT64_MAX-1, UINT64_C(1) << 63};
    for (unsigned n = 0; n < 3; ++n) if (strength(edge[n]) != edge[n]) return 2;
    return 0;
}
