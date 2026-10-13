#include <stdint.h>

void fill_squares(int32_t *p, int64_t n) {
    for (int64_t i = 0; i < n; i++) p[i] = (int32_t)(i * i);
}
