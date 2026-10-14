/* Integer arithmetic: promotions, the usual arithmetic conversions,
 * signedness, shifts, division, narrowing and widening. */
#include <stdio.h>
#include <limits.h>
#include <stdint.h>

static volatile int vzero = 0; /* defeats constant folding */

int main(void) {
    signed char sc = -5;
    unsigned char uc = 250;
    short s = -300;
    unsigned short us = 65000;
    int i = -7;
    unsigned u = 3000000000u;
    long l = -1234567890123L;
    unsigned long ul = 18000000000000000000ul;
    long long ll = LLONG_MIN + vzero;
    char c = '\xff';

    printf("%d %d %d %d\n", sc + uc, uc * uc, s * us, (unsigned char)(uc + 10));
    printf("%d %u %d\n", i / 2, u / 7, i % 3);
    printf("%d %d\n", -7 / 2 + vzero, -7 % 2 + vzero);
    printf("%u %d %ld\n", u + i, u > (unsigned)i, (long)u * 2);
    printf("%d %d\n", i < u, (long)i < (long)u); /* -7 converts to unsigned */
    printf("%ld %lu %lld\n", l / 1000, ul / 3, ll + 1);
    printf("%d %d %d\n", c, (unsigned char)c, c == -1);
    printf("%d %u %d\n", i >> 1, u >> 3, 1 << 30);
    printf("%lu %ld\n", ul >> 60, l >> 40);
    printf("%d %d %d\n", (short)70000, (signed char)200, (unsigned short)-1);
    printf("%d %d\n", sizeof(long double) == sizeof(double), (int)sizeof(sc + sc));
    printf("%x %x %x\n", ~0u, ~(unsigned char)0, (unsigned)~s);
    printf("%d %d %d %d\n", !i, !0, -i, +uc);
    printf("%d %d\n", INT_MAX + vzero > 0, UINT_MAX + vzero + 1u);

    int x = 10;
    x += 5; x -= 3; x *= 4; x /= 3; x %= 7; x <<= 4; x >>= 2; x &= 0x3c; x |= 1; x ^= 0xff;
    printf("%d\n", x);
    unsigned char b = 200;
    b += 100; /* wraps */
    short sh = 30000;
    sh += 30000;
    printf("%d %d\n", b, sh);
    int k = 5;
    int pre = ++k, post = k++;
    int dec = k--;
    printf("%d %d %d %d\n", pre, post, dec, --k);
    _Bool t = 42, f = 0.0, p = &k;
    t++;
    f--;
    printf("%d %d %d\n", t, f, p);

    __int128 big = (__int128)1 << 100;
    unsigned __int128 ubig = ~(unsigned __int128)0;
    big = big / 3 + (big >> 99);
    printf("%llx %llx %d\n", (unsigned long long)(big >> 64), (unsigned long long)ubig,
           big > 0);

    uint32_t h = 2166136261u;
    for (const char *p2 = "keel"; *p2; p2++) {
        h ^= (unsigned char)*p2;
        h *= 16777619u;
    }
    printf("%08x\n", h);
    return 0;
}
