#include <stdint.h>
#include <stdio.h>

#include "clobber.h"

int64_t pressure(int64_t);
int64_t pressure_call(int64_t);

static int64_t model(int64_t x) {
    uint64_t a[24], h = 0;
    for (int i = 0; i < 24; i++) a[i] = (uint64_t)x + (uint64_t)(i * i + 1);
    for (int i = 23; i >= 0; i--) h = h * 3 + a[i];
    return (int64_t)h;
}

int main(void) {
    int64_t xs[] = {0, 1, -5, 1LL << 40};
    int failures = 0;
    for (int i = 0; i < 4; i++) {
        int64_t want = model(xs[i]);
        int64_t got = pressure(xs[i]), got_call = pressure_call(xs[i]);
        if (got != want) {
            printf("pressure(%lld) = %lld, want %lld\n", xs[i], got, want);
            failures++;
        }
        if (got_call != want) {
            printf("pressure_call(%lld) = %lld, want %lld\n", xs[i], got_call, want);
            failures++;
        }
    }
    /* An exit status is taken mod 256. */
    return failures != 0;
}
