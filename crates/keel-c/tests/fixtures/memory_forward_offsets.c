extern int printf(const char *, ...);
struct Pair { unsigned a, b; };
__attribute__((noinline)) unsigned fields(struct Pair *p) {
    unsigned x = p->a;
    p->b = 17;
    return x + p->a;
}
__attribute__((noinline)) unsigned derived(unsigned *restrict p, unsigned *restrict q) {
    unsigned x = p[1];
    q[1] = 29;
    return x + p[1];
}
__attribute__((noinline)) unsigned ordinary(unsigned *p, unsigned *q) {
    unsigned x = p[1];
    q[1] = 31;
    return x + p[1];
}
__attribute__((noinline)) unsigned mixed(unsigned *p) {
    unsigned x = *p;
    ((unsigned char *)p)[1] = 7;
    return x + *p;
}
__attribute__((noinline)) unsigned unknown(unsigned *p, unsigned n) {
    unsigned x = p[0];
    p[n] = 37;
    return x + p[0];
}
__attribute__((noinline)) unsigned readonly(const unsigned *restrict p, const unsigned *restrict q) {
    return p[1] + q[1];
}
int main(void) {
    for (unsigned i = 0; i < 16; ++i) {
        struct Pair p = { i + 1, i + 3 };
        unsigned a[3] = { i + 5, i + 9, i + 11 };
        unsigned b[3] = { i + 7, i + 13, i + 17 };
        printf("%u ", fields(&p));
        printf("%u ", derived(a, b));
        printf("%u ", ordinary(a, a));
        printf("%u ", mixed(a));
        printf("%u ", unknown(a, i % 3));
        printf("%u %u %u %u %u\n", readonly(a, a), p.a, p.b, a[0], a[1]);
    }
    return 0;
}
