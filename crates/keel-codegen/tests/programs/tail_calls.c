#include <stdint.h>
#include <stdio.h>
#include "clobber.h"

int64_t k_sum(int64_t n, int64_t acc);
int32_t k_even(int64_t n);
int64_t k_indirect(void *self, int64_t n);
int64_t k_to_c(int64_t x);
int64_t k_to_c_indirect(int64_t (*f)(int64_t, int64_t, int64_t, int64_t, int64_t, int64_t,
                                     int64_t, int64_t),
                        int64_t x);
int8_t k_narrow(int8_t x);

int64_t c_sum8(int64_t a, int64_t b, int64_t c, int64_t d, int64_t e, int64_t f, int64_t g,
               int64_t h) {
    return a + 2 * b + 3 * c + 4 * d + 5 * e + 6 * f + 7 * g + 8 * h;
}

int main(void) {
    int failures = 0;
    int64_t n = 10000000;
    if (k_sum(n, 0) != n * (n + 1) / 2) {
        printf("k_sum = %lld\n", (long long)k_sum(n, 0));
        failures++;
    }
    if (k_even(n) != 1 || k_even(n + 1) != 0) {
        printf("k_even wrong\n");
        failures++;
    }
    if (k_indirect((void *)k_indirect, n) != 77) {
        printf("k_indirect wrong\n");
        failures++;
    }
    int64_t want = 0;
    for (int i = 0; i < 8; i++) want += (i + 1) * (10 + i);
    if (k_to_c(10) != want) {
        printf("k_to_c(10) = %lld, want %lld\n", (long long)k_to_c(10), (long long)want);
        failures++;
    }
    if (k_to_c_indirect(c_sum8, 3) != 36 * 3) {
        printf("k_to_c_indirect wrong\n");
        failures++;
    }
    /* Through a volatile pointer, so C does not assume the extension. */
    int8_t (*volatile narrow)(int8_t) = k_narrow;
    int32_t wide = ((int32_t (*)(int8_t))narrow)(5);
    if (wide != -5) {
        printf("k_narrow(5) = %d as i32\n", wide);
        failures++;
    }
    /* An exit status is taken mod 256. */
    return failures != 0;
}
