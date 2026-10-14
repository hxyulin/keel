#include <stdint.h>
#include <stdio.h>

int64_t k_setjmp(int64_t x);
int64_t k_setjmp_swapped(int64_t x);

int main(void) {
    int failures = 0;
    int64_t xs[] = {1, 7, -3, 1000003};
    for (int i = 0; i < 4; i++) {
        int64_t x = xs[i];
        int64_t a = k_setjmp(x), b = k_setjmp_swapped(x);
        if (a != 326 * x || b != 326 * x) {
            printf("x = %lld: got %lld and %lld, want %lld\n", (long long)x, (long long)a,
                   (long long)b, (long long)(326 * x));
            failures++;
        }
    }
    return failures;
}
