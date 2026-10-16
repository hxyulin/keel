extern unsigned long effect(unsigned long);
unsigned long dependent(const unsigned long *p, unsigned long n, unsigned long s) {
    for (unsigned long i = 0; i < n; ++i) s += p[i] * s;
    return s;
}
double floating(const double *p, unsigned long n, double s) {
    for (unsigned long i = 0; i < n; ++i) s += p[i];
    return s;
}
unsigned long volatile_load(const volatile unsigned long *p, unsigned long n) {
    unsigned long s = 0;
    for (unsigned long i = 0; i < n; ++i) s += p[i];
    return s;
}
unsigned long calls(const unsigned long *p, unsigned long n) {
    unsigned long s = 0;
    for (unsigned long i = 0; i < n; ++i) s += effect(p[i]);
    return s;
}
unsigned long stores(unsigned long *p, unsigned long n) {
    unsigned long s = 0;
    for (unsigned long i = 0; i < n; ++i) { s += p[i]; p[i] = s; }
    return s;
}
unsigned long early_exit(const unsigned long *p, unsigned long n) {
    unsigned long s = 0;
    for (unsigned long i = 0; i < n; ++i) { if (!p[i]) return s; s += p[i]; }
    return s;
}
