#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

double k_fma_f64(double a, double b, double c);
float k_fma_f32(float a, float b, float c);
double k_copysign_f64(double a, double b);
float k_copysign_f32(float a, float b);
double k_const_fma(void);
int64_t k_const_copysign(void);
int32_t k_const_copysign_f32(void);

static int failures;

static uint64_t bits64(double x) {
    uint64_t b;
    memcpy(&b, &x, 8);
    return b;
}

static uint32_t bits32(float x) {
    uint32_t b;
    memcpy(&b, &x, 4);
    return b;
}

static double nan_payload(uint64_t bits) {
    double x;
    memcpy(&x, &bits, 8);
    return x;
}

/* Results compare by bits, except that any NaN matches any NaN for fma:
   which NaN an operation makes is unspecified. */
static int same64(double got, double want) {
    return (isnan(got) && isnan(want)) || bits64(got) == bits64(want);
}

static int same32(float got, float want) {
    return (isnan(got) && isnan(want)) || bits32(got) == bits32(want);
}

int main(void) {
    double xs[] = {0.0, -0.0, 1.0, -1.0, 0.1, 10.0, 3.0, 1e308, -1e308, 5e-324,
                   INFINITY, -INFINITY, NAN, nan_payload(0xfff8000000000abc),
                   1.0 / 3.0, 0x1.fffffffffffffp1023};
    int n = sizeof xs / sizeof xs[0];
    for (int i = 0; i < n; i++)
        for (int j = 0; j < n; j++) {
            double a = xs[i], b = xs[j];
            /* copysign is exact on bits, NaN payloads included. */
            if (bits64(k_copysign_f64(a, b)) != bits64(copysign(a, b))) {
                printf("copysign(%a, %a) = %a\n", a, b, k_copysign_f64(a, b));
                failures++;
            }
            float fa = (float)a, fb = (float)b;
            if (bits32(k_copysign_f32(fa, fb)) != bits32(copysignf(fa, fb))) {
                printf("copysignf(%a, %a) = %a\n", fa, fb, k_copysign_f32(fa, fb));
                failures++;
            }
            for (int k = 0; k < n; k++) {
                double c = xs[k];
                if (!same64(k_fma_f64(a, b, c), fma(a, b, c))) {
                    printf("fma(%a, %a, %a) = %a, want %a\n", a, b, c, k_fma_f64(a, b, c),
                           fma(a, b, c));
                    failures++;
                }
                float fc = (float)c;
                if (!same32(k_fma_f32(fa, fb, fc), fmaf(fa, fb, fc))) {
                    printf("fmaf(%a, %a, %a) = %a, want %a\n", fa, fb, fc,
                           k_fma_f32(fa, fb, fc), fmaf(fa, fb, fc));
                    failures++;
                }
            }
        }
    if (k_const_fma() != fma(0.1, 10.0, -1.0) || k_const_fma() == 0.0) {
        printf("k_const_fma() = %a\n", k_const_fma());
        failures++;
    }
    if ((uint64_t)k_const_copysign() != 0xfff8000000000123) {
        printf("k_const_copysign() = %#llx\n", (unsigned long long)k_const_copysign());
        failures++;
    }
    if ((uint32_t)k_const_copysign_f32() != 0x7fc00042) {
        printf("k_const_copysign_f32() = %#x\n", (unsigned)k_const_copysign_f32());
        failures++;
    }
    /* An exit status is taken mod 256. */
    return failures != 0;
}
