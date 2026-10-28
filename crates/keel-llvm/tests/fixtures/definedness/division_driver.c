#include <stdint.h>
#include <stdio.h>
extern int64_t division_masked(int64_t, int64_t);
int main(void) {
    const int64_t inputs[] = {INT64_MIN, -2, -1, 0, 1, INT64_MAX - 1, INT64_MAX};
    for (unsigned i = 0; i < sizeof(inputs) / sizeof(inputs[0]); ++i) {
        int64_t x = inputs[i];
        if (division_masked(x, 0) != 7) return 1;
        if (x != INT64_MAX && division_masked(x, 1) != -(x + 1)) return 1;
    }
    puts("division-definedness-ok");
    return 0;
}
