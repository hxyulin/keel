#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

double poly(double);
float poly32(float);
double mixed(int64_t, double, int32_t, float, const double *);
double call_c(double);
void unary(double, double *);
void unary32(float, float, float *);
void minmax(double, double, double *);
int32_t compare(double, double);
int32_t compare32(float, float);
void to_int(double, int64_t *);
int32_t to_int32(float);
void from_int(int8_t, int64_t, double *);
int64_t bits(double);
float from_bits32(int32_t);
float narrow(double);
double neg_zero(void);
double snan(void);
double tiny(void);
float tenth32(void);
double select_f(int32_t, double, double);
double harmonic(int64_t);
double swaps(double, double, int64_t);
double pressure(double);
void consts(double *);

/* Overwrites every caller-saved FP register (v0-v7, v16-v31), as any call
 * may. */
void fclobber(void) {
    __asm__ volatile(
        "movi v0.16b, #0x55\n\tmovi v1.16b, #0x55\n\tmovi v2.16b, #0x55\n\t"
        "movi v3.16b, #0x55\n\tmovi v4.16b, #0x55\n\tmovi v5.16b, #0x55\n\t"
        "movi v6.16b, #0x55\n\tmovi v7.16b, #0x55\n\tmovi v16.16b, #0x55\n\t"
        "movi v17.16b, #0x55\n\tmovi v18.16b, #0x55\n\tmovi v19.16b, #0x55\n\t"
        "movi v20.16b, #0x55\n\tmovi v21.16b, #0x55\n\tmovi v22.16b, #0x55\n\t"
        "movi v23.16b, #0x55\n\tmovi v24.16b, #0x55\n\tmovi v25.16b, #0x55\n\t"
        "movi v26.16b, #0x55\n\tmovi v27.16b, #0x55\n\tmovi v28.16b, #0x55\n\t"
        "movi v29.16b, #0x55\n\tmovi v30.16b, #0x55\n\tmovi v31.16b, #0x55"
        :
        :
        : "v0", "v1", "v2", "v3", "v4", "v5", "v6", "v7", "v16", "v17", "v18",
          "v19", "v20", "v21", "v22", "v23", "v24", "v25", "v26", "v27", "v28",
          "v29", "v30", "v31");
}

double c_mix(int64_t a, double b, float c, int32_t d) {
    return (double)a * 1000 + b * 100 + (double)c * 10 + d;
}

static int failures;

static uint64_t dbits(double x) {
    uint64_t u;
    memcpy(&u, &x, 8);
    return u;
}

static uint32_t fbits(float x) {
    uint32_t u;
    memcpy(&u, &x, 4);
    return u;
}

/* Bit-exact: tells -0 from +0 and compares NaNs by payload. */
#define CHECK_BITS(got, want, bitsfn)                                         \
    do {                                                                      \
        if (bitsfn(got) != bitsfn(want)) {                                    \
            printf("line %d: %s = %a, want %a\n", __LINE__, #got,             \
                   (double)(got), (double)(want));                            \
            failures++;                                                       \
        }                                                                     \
    } while (0)
#define CHECK_D(got, want) CHECK_BITS(got, want, dbits)
#define CHECK_F(got, want) CHECK_BITS(got, want, fbits)
#define CHECK_I(got, want)                                                    \
    do {                                                                      \
        long long got_ = (long long)(got), want_ = (long long)(want);         \
        if (got_ != want_) {                                                  \
            printf("line %d: %s = %lld, want %lld\n", __LINE__, #got, got_,   \
                   want_);                                                    \
            failures++;                                                       \
        }                                                                     \
    } while (0)

/* IEEE minimum/maximum, which C's fmin/fmax are not (they drop NaNs). */
static double model_min(double a, double b) {
    if (isnan(a) || isnan(b)) return NAN;
    if (a == b) return signbit(a) ? a : b;
    return a < b ? a : b;
}
static double model_max(double a, double b) {
    if (isnan(a) || isnan(b)) return NAN;
    if (a == b) return signbit(a) ? b : a;
    return a > b ? a : b;
}

static int model_compare(double a, double b) {
    int uno = isnan(a) || isnan(b);
    return (a == b) | (a != b) << 1 | (a < b) << 2 | (a <= b) << 3 |
           (a > b) << 4 | (a >= b) << 5 | !uno << 6 | uno << 7;
}

/* Saturating conversions; C leaves out-of-range casts undefined. */
static int64_t sat_i64(double x, double lo, double hi, int64_t min, int64_t max) {
    if (isnan(x)) return 0;
    if (x <= lo) return min;
    if (x >= hi) return max;
    return (int64_t)x;
}
static uint64_t sat_u64(double x, double hi, uint64_t max) {
    if (isnan(x) || x <= 0) return 0;
    if (x >= hi) return max;
    return (uint64_t)x;
}

/* Calls `pressure` with known values in d8-d15 and returns how many of them
 * survived. keel must save and restore the callee-saved FP registers it
 * uses. */
static int callee_saved_kept(void) {
    double out[8];
    __asm__ volatile(
        "mov x19, %0\n\t"
        "fmov d8, #1.0\n\tfmov d9, #2.0\n\tfmov d10, #3.0\n\tfmov d11, #4.0\n\t"
        "fmov d12, #5.0\n\tfmov d13, #6.0\n\tfmov d14, #7.0\n\tfmov d15, #8.0\n\t"
        "fmov d0, #0.5\n\t"
        "bl _pressure\n\t"
        "stp d8, d9, [x19]\n\tstp d10, d11, [x19, #16]\n\t"
        "stp d12, d13, [x19, #32]\n\tstp d14, d15, [x19, #48]"
        :
        : "r"(out)
        : "x0", "x1", "x2", "x3", "x4", "x5", "x6", "x7", "x8", "x9", "x10",
          "x11", "x12", "x13", "x14", "x15", "x16", "x17", "x19", "x30", "v0",
          "v1", "v2", "v3", "v4", "v5", "v6", "v7", "v8", "v9", "v10", "v11",
          "v12", "v13", "v14", "v15", "v16", "v17", "v18", "v19", "v20", "v21",
          "v22", "v23", "v24", "v25", "v26", "v27", "v28", "v29", "v30", "v31",
          "memory", "cc");
    int kept = 0;
    for (int i = 0; i < 8; i++) kept += out[i] == i + 1;
    return kept;
}

static double model_pressure(double x) {
    double v[21];
    v[0] = x;
    for (int i = 1; i <= 20; i++) v[i] = (i == 1 ? x : v[i - 1]) + 1.0;
    /* v[i] here is keel's v(i+1): v2 = x + 1 ... */
    double a = v[1] * v[2];
    a = a + v[3];
    a = a * v[4];
    a = a + v[5];
    a = a - v[6];
    a = a + v[7];
    a = a - v[8];
    a = a + v[9];
    a = a * v[10];
    a = a + v[11];
    a = a - v[12];
    a = a + v[13];
    a = a + v[14];
    a = a - v[15];
    a = a + v[16];
    a = a + v[17];
    a = a - v[18];
    a = a + v[19];
    return a + x;
}

int main(void) {
    double xs[] = {0.0, -0.0, 1.5, -2.25, 1e10, 3.0e-310, INFINITY, -INFINITY, NAN};
    for (int i = 0; i < 9; i++) {
        double x = xs[i];
        if (!isnan(x)) CHECK_D(poly(x), 3 * (x * x) - 2 * x + 0.5);
        float f = (float)x;
        if (!isnan(f)) CHECK_F(poly32(f), 3.0f * (f * f) - 2.0f * f + 0.5f);

        double u[7];
        unary(x, u);
        if (!isnan(x)) {
            CHECK_D(u[0], sqrt(x));
            CHECK_D(u[1], ceil(x));
            CHECK_D(u[2], floor(x));
            CHECK_D(u[3], trunc(x));
            CHECK_D(u[4], nearbyint(x));
            CHECK_D(u[5], -x);
            CHECK_D(u[6], fabs(x));
        } else {
            for (int k = 0; k < 7; k++) CHECK_I(isnan(u[k]), 1);
        }

        int64_t n[4];
        to_int(x, n);
        CHECK_I(n[0], sat_i64(x, -2147483648.0, 2147483647.0, INT32_MIN, INT32_MAX));
        CHECK_I(n[1], sat_i64(x, -9223372036854775808.0, 9223372036854775808.0, INT64_MIN,
                              INT64_MAX));
        CHECK_I(n[2], sat_u64(x, 4294967295.0, UINT32_MAX));
        CHECK_I(n[3], (int64_t)sat_u64(x, 18446744073709551616.0, UINT64_MAX));

        for (int j = 0; j < 9; j++) {
            double y = xs[j];
            double m[2];
            minmax(x, y, m);
            if (isnan(x) || isnan(y)) {
                CHECK_I(isnan(m[0]) && isnan(m[1]), 1);
            } else {
                CHECK_D(m[0], model_min(x, y));
                CHECK_D(m[1], model_max(x, y));
            }
            CHECK_I(compare(x, y), model_compare(x, y));
            float fx = (float)x, fy = (float)y;
            int uno = isnan(fx) || isnan(fy);
            CHECK_I(compare32(fx, fy), (fx < fy) | uno << 1);
        }
    }

    float u32[4];
    unary32(2.0f, 3.0f, u32);
    CHECK_F(u32[0], sqrtf(2.0f));
    CHECK_F(u32[1], 2.0f);
    CHECK_F(u32[2], -2.0f);
    CHECK_F(u32[3], 2.0f / 3.0f);
    unary32(2.5f, 0.0f, u32);
    CHECK_F(u32[1], 2.0f);
    CHECK_F(u32[3], INFINITY);
    CHECK_I(to_int32(-7.9f), -7);
    CHECK_I(to_int32(3e9f), INT32_MAX);

    double p = 0.25;
    CHECK_D(mixed(5, 1.5, -3, 2.5f, &p), 5.0 + 1.5 * 2 + -3.0 * 3 + 2.5 * 4 + 0.25);
    CHECK_D(call_c(2.0), 2.0 + c_mix(7, 2.0, 1.5f, -2) + 20.0);

    int8_t bs[] = {0, 1, -1, 127, -128};
    int64_t ws[] = {0, -1, 1LL << 53, (1LL << 53) + 1, INT64_MIN, 0x7fffffffffffffffLL};
    for (int i = 0; i < 5; i++) {
        for (int j = 0; j < 6; j++) {
            double r[5];
            from_int(bs[i], ws[j], r);
            CHECK_D(r[0], (double)bs[i]);
            CHECK_D(r[1], (double)(uint8_t)bs[i]);
            CHECK_D(r[2], (double)ws[j]);
            CHECK_D(r[3], (double)(uint64_t)ws[j]);
            CHECK_D(r[4], (double)(float)(uint64_t)ws[j]);
        }
    }

    CHECK_I(bits(1.0), 0x3ff0000000000000LL);
    CHECK_I(bits(-0.0), (int64_t)0x8000000000000000ULL);
    CHECK_F(from_bits32(0x40490fdb), 3.14159274f);
    CHECK_F(narrow(0.1), 0.1f);
    CHECK_F(narrow(1e300), INFINITY);
    CHECK_I(dbits(neg_zero()), (int64_t)0x8000000000000000ULL);
    CHECK_I(dbits(snan()), 0x7ff0000000000001LL);
    CHECK_I(dbits(tiny()), 1);
    CHECK_F(tenth32(), 0.1f);
    CHECK_D(select_f(1, 1.5, 2.5), 1.5);
    CHECK_D(select_f(0, 1.5, 2.5), 2.5);

    for (int64_t n = 0; n <= 10; n++) {
        double want = 0;
        for (int64_t i = 1; i <= n; i++) want += 1.0 / (double)i;
        CHECK_D(harmonic(n), want);
    }
    CHECK_D(swaps(1.0, 2.0, 0), 12.0);
    CHECK_D(swaps(1.0, 2.0, 1), 21.0);
    CHECK_D(swaps(1.0, 2.0, 7), 21.0);
    CHECK_D(swaps(1.0, 2.0, 8), 12.0);
    CHECK_D(pressure(0.5), model_pressure(0.5));
    CHECK_D(pressure(-3.25), model_pressure(-3.25));
    CHECK_I(callee_saved_kept(), 8);

    /* Computed here at run time, from volatile inputs, by the target. */
    volatile double tenth = 0.1, fifth = 0.2, one = 1.0, three = 3.0, two = 2.0,
                    m25 = -2.5, zero = 0.0, nzero = -0.0, big = 1e300;
    volatile float f24 = 16777216.0f, f1 = 1.0f;
    volatile uint64_t all = UINT64_MAX;
    double c[10];
    consts(c);
    CHECK_D(c[0], tenth + fifth);
    CHECK_D(c[1], one / three);
    CHECK_D(c[2], sqrt(two));
    CHECK_D(c[3], nearbyint(m25));
    CHECK_D(c[4], model_min(zero, nzero));
    CHECK_D(c[5], big * big);
    CHECK_I(dbits(c[6]), INT64_MAX);
    CHECK_D(c[7], (double)(float)(f24 + f1));
    CHECK_D(c[8], (double)all);
    CHECK_I(dbits(c[9]), (zero < nzero) + 2 * (big * big != big * big));
    /* An exit status is taken mod 256. */
    return failures != 0;
}
