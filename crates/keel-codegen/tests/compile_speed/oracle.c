#include <stdint.h>
extern uint64_t arithmetic(uint64_t, uint64_t);
extern uint64_t call_swap(uint64_t, uint64_t);
extern uint64_t memory(uint64_t *, uint64_t);
int main(void) {
    for (uint64_t y = 0; y < 64; ++y) {
        uint64_t x = y * UINT64_C(0x9e3779b97f4a7c15);
        uint64_t a = x ^ UINT64_C(0x8000000000000000);
        uint64_t b = (((a + y - x) * y) & a) | x;
        if (arithmetic(x, y) != (b << y) >> y) return 1;
        if (call_swap(y, x) != ((y & 1) ? (uint64_t)-13 : x)) return 2;
    }
    for (unsigned n = 0; n <= 16; ++n) {
        uint64_t a[16], expected = 0;
        for (unsigned j = 0; j < 16; ++j) a[j] = j * 17 + 5;
        for (unsigned j = 0; j < n; ++j) expected += ~a[j];
        if (memory(a, n) != expected) return 3;
        for (unsigned j = 0; j < 16; ++j)
            if (a[j] != (j < n ? ~(uint64_t)(j * 17 + 5) : (uint64_t)(j * 17 + 5))) return 4;
    }
    if (check_comparisons()) return 5;
    return 0;
}
