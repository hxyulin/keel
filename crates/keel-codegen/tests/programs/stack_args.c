#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

typedef double many_fn(int64_t, int64_t, int64_t, int64_t, int64_t, int64_t, int64_t,
                       int64_t, double, double, double, double, double, double, double,
                       double, int32_t, int64_t, int32_t, float, double, int32_t);

many_fn k_many;
double k_call_many(int64_t, many_fn *);
double k_call_c_many(int64_t);
void k_print(void);
int64_t k_vsum(void);
int64_t k_vtail(void);
int64_t k_call_small(int64_t);

double c_many(int64_t a0, int64_t a1, int64_t a2, int64_t a3, int64_t a4, int64_t a5,
              int64_t a6, int64_t a7, double d0, double d1, double d2, double d3, double d4,
              double d5, double d6, double d7, int32_t s0, int64_t s1, int32_t s2, float s3,
              double s4, int32_t s5) {
    int64_t a[] = {a0, a1, a2, a3, a4, a5, a6, a7};
    double d[] = {d0, d1, d2, d3, d4, d5, d6, d7};
    int64_t ints = 0;
    for (int i = 0; i < 8; i++) ints += a[i] * (i + 1);
    double floats = 0;
    for (int i = 0; i < 8; i++) floats = floats + d[i] * (i + 11);
    double r = (double)ints + floats;
    r = r + (double)s0 * 101;
    r = r + (double)s1 * 103;
    r = r + (double)s2 * 107;
    r = r + (double)s3 * 109;
    r = r + s4 * 113;
    r = r + (double)s5 * 127;
    return r;
}

/* Reads variadic arguments by the letters in `types`: i int, l int64_t,
 * p pointer, d double. Weights each by its position so a misplaced one
 * shows. */
int64_t c_vsum(const char *types, ...) {
    va_list ap;
    va_start(ap, types);
    int64_t sum = 0;
    for (int i = 0; types[i]; i++) {
        int64_t w = i + 1, x;
        switch (types[i]) {
        case 'i': x = va_arg(ap, int); break;
        case 'l': x = va_arg(ap, int64_t); break;
        case 'p': x = va_arg(ap, const char *) == types ? 1000 : -1000; break;
        default: x = (int64_t)(va_arg(ap, double) * 100); break;
        }
        sum += w * x;
    }
    va_end(ap);
    return sum;
}

/* After eight named integers and a named int on the stack: an int64_t, a
 * double and an int, variadic. */
int64_t c_vtail(int64_t a0, int64_t a1, int64_t a2, int64_t a3, int64_t a4, int64_t a5,
                int64_t a6, int64_t a7, int32_t s0, ...) {
    va_list ap;
    va_start(ap, s0);
    int64_t l = va_arg(ap, int64_t);
    double d = va_arg(ap, double);
    int i = va_arg(ap, int);
    va_end(ap);
    return a0 + a1 + a2 + a3 + a4 + a5 + a6 + a7 + s0 * 10 + l * 100 + (int64_t)(d * 1000) +
           i * 10000;
}

static int failures;

static double model(int64_t x) {
    double dx = (double)x, q = dx * 0.25;
    double d[8];
    d[0] = q;
    for (int i = 1; i < 8; i++) d[i] = d[i - 1] + q;
    int32_t x32 = (int32_t)x;
    return c_many(x, x + 1, x + 2, x + 3, x + 4, x + 5, x + 6, x + 7, d[0], d[1], d[2], d[3],
                  d[4], d[5], d[6], d[7], -x32, (int64_t)((uint64_t)x << 33), 7 - x32,
                  (float)dx * 0.125f, dx * 1.5, x32 * 1000);
}

#define CHECK(got, want)                                                      \
    do {                                                                      \
        double got_ = (got), want_ = (want);                                  \
        if (memcmp(&got_, &want_, 8) != 0) {                                  \
            printf("line %d: %s = %.17g, want %.17g\n", __LINE__, #got, got_, \
                   want_);                                                    \
            failures++;                                                       \
        }                                                                     \
    } while (0)

int main(void) {
    int64_t xs[] = {0, 1, -3, 1000, -123456};
    for (int i = 0; i < 5; i++) {
        int64_t x = xs[i];
        CHECK(k_call_c_many(x), model(x));           /* keel -> C */
        CHECK(k_call_many(x, k_many), model(x));     /* keel -> keel */
        double dx = (double)x, q = dx * 0.25;       /* C -> keel */
        int32_t x32 = (int32_t)x;
        CHECK(k_many(x, x + 1, x + 2, x + 3, x + 4, x + 5, x + 6, x + 7, q, q * 2, q * 3, q * 4,
                     q * 5, q * 6, q * 7, q * 8, -x32, (int64_t)((uint64_t)x << 33), 7 - x32,
                     (float)dx * 0.125f, dx * 1.5, x32 * 1000),
              c_many(x, x + 1, x + 2, x + 3, x + 4, x + 5, x + 6, x + 7, q, q * 2, q * 3, q * 4,
                     q * 5, q * 6, q * 7, q * 8, -x32, (int64_t)((uint64_t)x << 33), 7 - x32,
                     (float)dx * 0.125f, dx * 1.5, x32 * 1000));
    }
    /* 1*1 + 2*250 + 3*3 + 4*1000 + 5*-4 + 6*25 + 7*-100 */
    CHECK((double)k_vsum(), 1 + 500 + 9 + 4000 - 20 + 150 - 700);
    CHECK((double)k_vtail(), 8 - 90 + 10000 + 500 + 30000);
    for (int i = 0; i < 5; i++) {
        int64_t x = xs[i];
        CHECK((double)k_call_small(x), (double)(-3 - 3000 + 5000 + (int64_t)(int32_t)x * 100000 + x));
    }
    k_print();
    /* An exit status is taken mod 256. */
    return failures != 0;
}
