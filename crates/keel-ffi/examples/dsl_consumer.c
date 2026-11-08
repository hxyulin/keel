/* Link this with the object produced from functions.dsl. */
#include <stdint.h>
#include <stdio.h>

extern int64_t score(int64_t x, int64_t y);
extern int64_t polynomial(int64_t x);
extern int64_t clamp(int64_t x, int64_t low, int64_t high);
extern int64_t distance(int64_t x, int64_t y);
extern int64_t answer(void);

int main(void) {
    int valid = score(8, 3) == 19 && score(3, 8) == 19
        && score(-8, -3) == -14 && score(4, 4) == 12
        && polynomial(5) == 24 && polynomial(-4) == 6
        && clamp(-9, -5, 7) == -5 && clamp(12, -5, 7) == 7
        && clamp(0, -5, 7) == 0 && clamp(-5, -5, 7) == -5
        && distance(-10, 4) == 14 && distance(4, -10) == 14
        && distance(3, 3) == 0 && answer() == 42;
    if (!valid) { fputs("DSL result mismatch\n", stderr); return 1; }
    puts("DSL functions passed");
    return 0;
}
