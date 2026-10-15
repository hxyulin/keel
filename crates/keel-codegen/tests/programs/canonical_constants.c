#include <stdint.h>
extern uint64_t canonical_constants(uint64_t);
extern uint64_t canonical_compare(uint64_t);
static int check(uint64_t x) {
    return canonical_compare(x) != ((uint64_t)(37 < (int64_t)x) | ((uint64_t)(37 >= x) << 1));
}
int main(void) {
    for (uint64_t x = 0; x < 1024; ++x) if (check(x) || canonical_constants(x) != ((x + 37) ^ 63)) return 1;
    uint64_t edge[] = {UINT64_MAX, UINT64_MAX-36, UINT64_C(1)<<63};
    for (unsigned i = 0; i < 3; ++i) if (check(edge[i]) || canonical_constants(edge[i]) != ((edge[i] + 37) ^ 63)) return 2;
    return 0;
}
