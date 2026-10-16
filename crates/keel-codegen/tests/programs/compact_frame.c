#include <stdint.h>
#include <stdlib.h>

extern int64_t compact_frame(int64_t);

int64_t touch_frame(int64_t *a, int64_t *b) {
    if ((uintptr_t)a % 16 || (uintptr_t)b % 8 ||
        !((uintptr_t)a + 24 <= (uintptr_t)b ||
          (uintptr_t)b + 8 <= (uintptr_t)a))
        abort();
    a[2] = a[0] + 1;
    a[0] = a[2];
    *b += 2;
    return 3;
}

int main(void) {
    return compact_frame(42) != 82;
}
