// Built with -O2, so `catcher` keeps its values in callee-saved registers
// across the call that throws.
#include <cstdio>

extern "C" long k_through(long (*callback)(long), long x, double y);

extern "C" long thrower(long x) {
    // A frame that calls through `k_through` normally returns this.
    if (x < 0) return 0;
    throw x;
}

__attribute__((noinline)) static int catcher(volatile long *ints, volatile double *floats) {
    long a0 = ints[0], a1 = ints[1], a2 = ints[2], a3 = ints[3], a4 = ints[4];
    long a5 = ints[5], a6 = ints[6], a7 = ints[7], a8 = ints[8], a9 = ints[9];
    double d0 = floats[0], d1 = floats[1], d2 = floats[2], d3 = floats[3];
    double d4 = floats[4], d5 = floats[5], d6 = floats[6], d7 = floats[7];
    long caught = 0;
    try {
        k_through(thrower, 7, 0.25);
    } catch (long e) {
        caught = e;
    }
    int failures = 0;
    if (caught != 7) {
        printf("caught %ld, want 7\n", caught);
        failures++;
    }
    long got[] = {a0, a1, a2, a3, a4, a5, a6, a7, a8, a9};
    for (int i = 0; i < 10; i++) {
        if (got[i] != ints[i]) {
            printf("int %d is %ld after unwinding, want %ld\n", i, got[i], (long)ints[i]);
            failures++;
        }
    }
    double fgot[] = {d0, d1, d2, d3, d4, d5, d6, d7};
    for (int i = 0; i < 8; i++) {
        if (fgot[i] != floats[i]) {
            printf("float %d is %g after unwinding, want %g\n", i, fgot[i], (double)floats[i]);
            failures++;
        }
    }
    return failures;
}

int main() {
    volatile long ints[10];
    volatile double floats[8];
    for (int i = 0; i < 10; i++) ints[i] = 1000 + 37 * i;
    for (int i = 0; i < 8; i++) floats[i] = 0.5 + 3 * i;
    int failures = catcher(ints, floats);
    // The call returns normally too: 10 * -1 + (1 + ... + 10), plus
    // 8 * -1.0 + (1.5 + ... + 8.5) truncated.
    long normal = k_through(thrower, -1, -1.0);
    if (normal != -10 + 55 + (long)(-8.0 + 40.0)) {
        printf("k_through(thrower, -1, -1.0) = %ld\n", normal);
        failures++;
    }
    /* An exit status is taken mod 256. */
    return failures != 0;
}
