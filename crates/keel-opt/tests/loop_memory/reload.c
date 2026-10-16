/* Realistic invariant restricted reload, matching the audited campaign. */
unsigned long kernel(unsigned long *restrict p, unsigned long *restrict q,
                     unsigned long n, unsigned long k) {
    unsigned long s = 0;
    for (unsigned long i = 0; i < n; ++i) {
        k = (k ^ (k >> 13)) * 6364136223846793005UL + 1;
        unsigned long a = *p;
        *q = k;
        s += (a + *p) ^ k;
    }
    return s;
}

unsigned long aliasing(unsigned long *p, unsigned long *q, unsigned long n) {
    unsigned long s = 0;
    for (unsigned long i = 0; i < n; ++i) {
        unsigned long a = *p;
        *q = i + 3;
        s += a;
    }
    return s;
}

unsigned long conditional(unsigned long *p, unsigned long n, unsigned long take) {
    unsigned long s = 0;
    for (unsigned long i = 0; i < n; ++i)
        if (take) s += *p;
    return s;
}

unsigned long readonly_alias(unsigned long *restrict p, unsigned long *restrict q,
                             unsigned long n) {
    unsigned long s = 0;
    for (unsigned long i = 0; i < n; ++i) s += *p + *q;
    return s;
}

unsigned long fault_order(unsigned long *restrict p, unsigned long *restrict q,
                          unsigned long n) {
    unsigned long s = 0;
    for (unsigned long i = 0; i < n; ++i) {
        *q = 123;
        s += *p;
    }
    return s;
}
