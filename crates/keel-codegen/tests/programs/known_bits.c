#include <stdint.h>
extern uint64_t known_bits(uint64_t);
extern uint64_t known_add(uint64_t);
int main(void) {
    for (uint64_t x = 0; x < 1024; ++x) if (known_add(x) != ((x&15)+16) || known_bits(x) != (x & 255)) return 1;
    uint64_t edge[] = {UINT64_MAX, UINT64_MAX-1, UINT64_C(1)<<63, (UINT64_C(1)<<63)-1};
    for (unsigned i = 0; i < sizeof(edge)/sizeof(edge[0]); ++i) if (known_add(edge[i]) != ((edge[i]&15)+16) || known_bits(edge[i]) != (edge[i] & 255)) return 2;
    return 0;
}
