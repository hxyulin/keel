#include <stdint.h>
extern uint64_t canonical_constants(uint64_t);
int main(void) {
    for (uint64_t x = 0; x < 1024; ++x) if (canonical_constants(x) != ((x + 37) ^ 63)) return 1;
    uint64_t edge[] = {UINT64_MAX, UINT64_MAX-36, UINT64_C(1)<<63};
    for (unsigned i = 0; i < 3; ++i) if (canonical_constants(edge[i]) != ((edge[i] + 37) ^ 63)) return 2;
    return 0;
}
