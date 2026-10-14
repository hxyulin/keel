#include <stdint.h>
extern uint64_t compare(uint64_t, uint64_t);
int main(void) {
    uint64_t inputs[] = {0, 1, UINT64_MAX, INT64_MAX, (uint64_t)INT64_MIN};
    for (unsigned i = 0; i < 5; ++i)
        for (unsigned j = 0; j < 5; ++j) {
            uint64_t a = inputs[i], b = inputs[j];
            uint64_t expected = 2 * ((int64_t)a < (int64_t)b) + 4 * (a < b);
            if (compare(a, b) != expected) return 1;
        }
    return 0;
}
