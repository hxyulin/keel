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

/* Independent optimizer oracle: each returns a constant i64 bit pattern.
 * Unsigned conversions and arithmetic are defined modulo the selected width. */
unsigned long long oracle_div(void) { return (unsigned long)-1 / 3; }
unsigned long long oracle_rem(void) { return (unsigned long)-1 % 7; }
unsigned long long oracle_shift(void) { return (unsigned long)-1 >> 31; }
unsigned long long oracle_compare(void) { return -1L < 1U; }
unsigned long long oracle_rank(void) { return (unsigned long)-1 + -2LL; }
unsigned long long oracle_cast(void) { return (unsigned long)0x100000001ULL; }
unsigned long long oracle_signed(void) { return -2147483647L / 3; }
unsigned long long oracle_bool(void) { return (_Bool)(unsigned long)0x100000000ULL; }
unsigned long long oracle_promote(void) { return (unsigned short)65535 + (signed char)-7; }

/* Runtime paths: these must preserve width, signedness, access and SSA shape. */
long long signed_widen(long x) { return x; }
long long signed_shift(long x) { return x >> 3; }
unsigned long long unsigned_shift(unsigned long x) { return x >> 3; }
long long slots(long x, long long y) {
    volatile signed char a = (signed char)x;
    volatile unsigned short b = (unsigned short)x;
    volatile long c = x;
    volatile long long d = y;
    c += b;
    d += a;
    return c + d;
}
long phi(long x) {
    long y;
    if (x) y = x + 1; else y = x - 1;
    return y;
}
int truth_not(long long x) { return !(long)x; }
int truth_and(long long x, int y) { return (long)x && y; }
