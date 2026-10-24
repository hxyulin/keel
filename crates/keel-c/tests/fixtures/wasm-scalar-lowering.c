/* Header-free private lowering fixture and independent Clang type oracle.
 * clang --target=wasm32-unknown-unknown -std=gnu11 -ffreestanding -nostdinc
 *   -Werror -fsyntax-only wasm-scalar-lowering.c
 * Also valid as a native LP64 control. */
_Static_assert(sizeof(long) == sizeof(void *), "long and pointer width");
_Static_assert(sizeof(long long) == 8, "long long width");
_Static_assert(__builtin_types_compatible_p(__typeof__(1U + -2L), unsigned long)
               == (sizeof(long) == 4), "mixed rank and width");
long helper(long x) { return x + 1; }
long widths(long x, unsigned long u, long long y) {
    volatile long slot = x;
    short s = (short)y;
    slot += s;
    return helper(slot) + (long)(u >> 31);
}
long folded(void) { return (long)0x100000001ULL; }
unsigned long mask(unsigned long x) { return x & (unsigned long)-1; }
unsigned long mixed(unsigned int x, long y) { return x / y; }
long long widen(unsigned long x, long long y) { return x + y; }
int truth(long long x) { if ((long)x) return 1; return 0; }
long choose(int x) { return x ? (long)0x100000001ULL : -1L; }
long control(long x) {
    long r = 0;
    for (long i = 0; i < x; i++) {
        switch (i) { case 2: continue; case 4: break; default: r += i; }
    }
    return r;
}
