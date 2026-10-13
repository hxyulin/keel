#include <stdint.h>
#include <stdio.h>

int64_t sum(const int32_t *p, int64_t n);

int main(void) {
    int32_t xs[] = {1, 2, 3, -4, 2147483647, 2147483647, -2147483647 - 1};
    int failures = 0;
    for (int64_t n = 0; n <= 7; n++) {
        int64_t want = 0;
        for (int64_t i = 0; i < n; i++) want += xs[i];
        int64_t got = sum(xs, n);
        if (got != want) {
            printf("sum(xs, %lld) = %lld, want %lld\n", n, got, want);
            failures++;
        }
    }
    return failures;
}
