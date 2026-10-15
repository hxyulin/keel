#include <stdint.h>
extern uint64_t scalar_simplify(uint64_t, uint64_t);
extern uint64_t scalar_casts(uint64_t);
extern uint64_t scalar_mixed_casts(uint64_t);
static int casts(uint64_t x) {
    return scalar_casts(x) != (uint8_t)x || scalar_mixed_casts(x) != (uint16_t)(int16_t)(int8_t)x;
}
extern uint64_t scalar_algebra(uint64_t, uint64_t);
int main(void) {
    uint64_t edge[] = {0, 1, 127, 128, 255, 256, UINT64_MAX, UINT64_C(1)<<63, (UINT64_C(1)<<63)-1};
    for (uint64_t x = 0; x < 256; ++x)
        for (uint64_t y = 0; y < 256; ++y)
            if (scalar_algebra(x,y) != y+44 || casts(x) || scalar_simplify(x,y) != (x^y)) return 1;
    for (unsigned x = 0; x < sizeof edge / sizeof *edge; ++x)
        for (unsigned y = 0; y < sizeof edge / sizeof *edge; ++y)
            if (scalar_algebra(edge[x],edge[y]) != edge[y]+44 || casts(edge[x]) || scalar_simplify(edge[x],edge[y]) != (edge[x]^edge[y])) return 2;
    return 0;
}
