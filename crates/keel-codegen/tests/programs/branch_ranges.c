#include <stdint.h>
extern uint64_t branch_ranges(uint64_t);
int main(void) {
    for (uint64_t x = 0; x < 1024; ++x) if (branch_ranges(x) != (x < 16)) return 1;
    uint64_t edge[] = {UINT64_MAX, UINT64_C(1)<<63, (UINT64_C(1)<<63)-1};
    for (unsigned i = 0; i < sizeof(edge)/sizeof(edge[0]); ++i) if (branch_ranges(edge[i]) != (edge[i] < 16)) return 2;
    return 0;
}
