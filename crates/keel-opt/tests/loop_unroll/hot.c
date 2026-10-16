/* Bounded typed variants of the measured hot reduction. Unsigned C wraps. */
unsigned long hot64(const unsigned long *p, unsigned long n,
                    unsigned long k, unsigned long seed) {
    unsigned long sum = seed;
    for (unsigned long i = 0; i < n; ++i)
        sum += (p[i] ^ k) * (i + 1);
    return sum;
}
unsigned hot32(const unsigned *p, unsigned n, unsigned k, unsigned seed) {
    unsigned sum = seed;
    for (unsigned i = 0; i < n; ++i)
        sum += (p[i] ^ k) * (i + 1);
    return sum;
}
