/* Small aggregate-free functions suitable for all three pipelines. */
unsigned long hot(unsigned long *p, unsigned long n, unsigned long k) {
    unsigned long sum = 0;
    for (unsigned long i = 0; i < n; ++i)
        sum += (p[i] ^ k) * (i + 1);
    return sum;
}
unsigned long bitmix(unsigned long x, unsigned long y) {
    return (x + y) * x - y;
}
int fp_compare(double x, double y) {
    return (x == y) + 2 * (x != y) + 4 * (x < y) + 8 * (x >= y);
}
signed char narrow_abi(signed char a, unsigned char b) {
    return (signed char)(a + b);
}
