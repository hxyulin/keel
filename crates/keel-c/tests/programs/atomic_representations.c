extern int printf(const char *, ...);
union F { unsigned u; float f; };
union D { unsigned long u; double d; };
struct __attribute__((aligned(8))) Pair { unsigned a, b; };
static _Atomic float f;
static _Atomic double d;
struct __attribute__((aligned(8))) Padded { char c; unsigned n; };
union P { struct Padded p; unsigned long bits; };
static struct Pair object, input, output;
static int calls;
static struct Pair *buffer(struct Pair *p) { ++calls; return p; }
static _Atomic float *fp(void) { ++calls; return &f; }
static double rhs(void) { ++calls; return 0.5; }
static int order(void) { ++calls; return 5; }
static int weak(void) { ++calls; return 1; }
int main(void) {
    {
        unsigned char v = 1, in = 2, out = 0, exp = 0;
        __atomic_load(&v, &out, 0);
        __atomic_store(&v, &in, 3);
        __atomic_exchange(&v, &out, &in, 4);
        int fail = __atomic_compare_exchange(&v, &exp, &in, 0, 2, 0);
        int pass = __atomic_compare_exchange(&v, &exp, &in, 0, 3, 0);
        printf("matrix %lu %d %d %lu %lu\n", (unsigned long)sizeof(unsigned char), fail, pass, (unsigned long)v, (unsigned long)exp);
    }
    {
        unsigned short v = 1, in = 2, out = 0, exp = 0;
        __atomic_load(&v, &out, 0);
        __atomic_store(&v, &in, 3);
        __atomic_exchange(&v, &out, &in, 4);
        int fail = __atomic_compare_exchange(&v, &exp, &in, 0, 2, 0);
        int pass = __atomic_compare_exchange(&v, &exp, &in, 0, 3, 0);
        printf("matrix %lu %d %d %lu %lu\n", (unsigned long)sizeof(unsigned short), fail, pass, (unsigned long)v, (unsigned long)exp);
    }
    {
        unsigned v = 1, in = 2, out = 0, exp = 0;
        __atomic_load(&v, &out, 0);
        __atomic_store(&v, &in, 3);
        __atomic_exchange(&v, &out, &in, 4);
        int fail = __atomic_compare_exchange(&v, &exp, &in, 0, 2, 0);
        int pass = __atomic_compare_exchange(&v, &exp, &in, 0, 3, 0);
        printf("matrix %lu %d %d %lu %lu\n", (unsigned long)sizeof(unsigned), fail, pass, (unsigned long)v, (unsigned long)exp);
    }
    {
        unsigned long v = 1, in = 2, out = 0, exp = 0;
        __atomic_load(&v, &out, 0);
        __atomic_store(&v, &in, 3);
        __atomic_exchange(&v, &out, &in, 4);
        int fail = __atomic_compare_exchange(&v, &exp, &in, 0, 2, 0);
        int pass = __atomic_compare_exchange(&v, &exp, &in, 0, 3, 0);
        printf("matrix %lu %d %d %lu %lu\n", (unsigned long)sizeof(unsigned long), fail, pass, (unsigned long)v, (unsigned long)exp);
    }
    union F nan = {0x7fc12345}, got, expected;
    __c11_atomic_store(&f, nan.f, 5);
    got.f = __c11_atomic_load(&f, 2);
    printf("nan %x\n", got.u);
    expected = nan;
    int ok = __c11_atomic_compare_exchange_strong(&f, &expected.f, -0.0f, 5, 2);
    got.f = f;
    printf("cas %d %x %x\n", ok, expected.u, got.u);
    expected.f = 0.0f;
    ok = __c11_atomic_compare_exchange_strong(&f, &expected.f, 2.0f, 5, 2);
    printf("zero %d %x\n", ok, expected.u);
    got.f = __c11_atomic_exchange(&f, 3.0f, 4);
    printf("exchange %x\n", got.u);
    float a = (*fp())++;
    float b = ++*fp();
    float c = (*fp() += rhs());
    f *= 2.0; f /= 4.0; f -= 0.25;
    printf("arith %.2f %.2f %.2f %.2f %d\n", (double)a, (double)b, (double)c, (double)f, calls);
    union D dn = {0x7ff8123456789abcUL}, de, dg;
    d = dn.d; de = dn;
    ok = __c11_atomic_compare_exchange_weak(&d, &de.d, -0.0, order(), order());
    dg.d = d;
    printf("double %d %lx %lx %d\n", ok, de.u, dg.u, calls);
    struct Pair p = {1, 2}, out = {0, 0}, val = {3, 4}, exp = {1, 2};
    __atomic_load(&p, &out, 2);
    printf("pair load %u %u\n", out.a, out.b);
    __atomic_store(&p, &val, 3);
    __atomic_exchange(&p, &exp, &out, 4);
    printf("pair exchange %u %u\n", out.a, out.b);
    ok = __atomic_compare_exchange(&p, &exp, &val, weak(), order(), order());
    printf("pair cas %d %u %u %d\n", ok, p.a, p.b, calls);
    ok = __atomic_compare_exchange(&p, &exp, &out, 0, 5, 2);
    printf("pair fail %d %u %u\n", ok, exp.a, exp.b);
    f = 16777216.0f;
    f += 1.00000001;
    printf("promotion %.0f\n", (double)f);
    f = nan.f;
    f += 1.0f;
    got.f = f;
    printf("nan arithmetic %x\n", got.u);
    union P padded = { .bits = 0x1234567801020304UL }, pd = { .bits = 0xfedcba9809080706UL }, pe = { .bits = 0 };
    __atomic_load(&padded.p, &pe.p, 5);
    printf("padding %lx\n", pe.bits);
    pe.bits ^= 0x100;
    ok = __atomic_compare_exchange(&padded.p, &pe.p, &pd.p, 0, 5, 2);
    printf("padding cas %d %lx\n", ok, pe.bits);
    int before = calls;
    __atomic_store(buffer(&object), buffer(&input), order());
    __atomic_load(buffer(&object), buffer(&output), order());
    __atomic_exchange(buffer(&object), buffer(&input), buffer(&output), order());
    __atomic_compare_exchange(buffer(&object), buffer(&output), buffer(&input), weak(), order(), order());
    printf("evaluations %d\n", calls - before);
    unsigned char x = 7, y = 8, z = 0;
    __atomic_exchange(&x, &y, &z, 5);
    unsigned short h = 123, hi = 321, ho = 0;
    __atomic_store(&h, &hi, 5); __atomic_load(&h, &ho, 5);
    unsigned w = 1, wi = 2, wo;
    __atomic_exchange(&w, &wi, &wo, 5);
    printf("widths %u %u %u %u %u\n", x, z, ho, w, wo);
    union F generic = nan, gin = {0x80000000}, gout, gexp = {0};
    __atomic_load(&generic.f, &gout.f, 2);
    printf("generic float load %x\n", gout.u);
    __atomic_store(&generic.f, &gin.f, 3);
    __atomic_exchange(&generic.f, &nan.f, &gout.f, 4);
    int gfail = __atomic_compare_exchange(&generic.f, &gexp.f, &gin.f, 0, 5, 2);
    int gpass = __atomic_compare_exchange(&generic.f, &gexp.f, &gin.f, 0, 5, 2);
    printf("generic float cas %x %d %d %x %x\n", gout.u, gfail, gpass, gexp.u, generic.u);
    {
        int n = calls ? 3 : 4;
        _Atomic float values[n];
        values[1] = 1.5f;
        float previous = values[1]++;
        values[1] *= rhs();
        printf("vla atomic %.2f %.2f\n", (double)previous, (double)values[1]);
    }
    return 0;
}
