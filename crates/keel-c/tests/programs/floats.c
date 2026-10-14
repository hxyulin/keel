/* Floating point: arithmetic, conversions to and from every integer
 * type, comparisons with NaN, float vs double rounding. */
#include <stdio.h>
#include <math.h>
#include <float.h>

static volatile double vd = 0.0;

double poly(double x) { return ((2.0 * x - 3.0) * x + 0.5) * x - 1.25; }
float fpoly(float x) { return (2.0f * x - 3.0f) * x + 0.5f; }

int main(void) {
    double a = 1.0 / 3.0, b = 1e308, c = -0.0;
    float f = 1.0f / 3.0f;
    printf("%.17g %.9g %.17g\n", a, f, (double)f);
    printf("%g %g %g\n", b * 10, -b * 10, c);
    double nan = vd / vd;
    printf("%d %d %d %d %d\n", nan == nan, nan != nan, nan < 1, isnan(nan), isinf(b * 10));
    printf("%d %u %ld %lu\n", (int)-3.9, (unsigned)3.9e9, (long)-1e18, (unsigned long)1.8e19);
    printf("%d %d %d\n", (signed char)100.7, (unsigned char)200.5, (short)-1234.5);
    printf("%.17g %.17g %.9g\n", (double)-7, (double)18446744073709551615ul, (float)16777217);
    printf("%.17g %.17g\n", (double)(long long)-9007199254740993ll, (double)4294967295u);
    printf("%.17g %.9g\n", poly(1.5), fpoly(2.25f));
    printf("%.17g %.17g %.17g\n", sqrt(2.0), floor(-2.5), fabs(-3.25));
    printf("%g %g %g\n", DBL_MAX, DBL_MIN, FLT_EPSILON);
    printf("%a %a\n", 0x1.8p3, 0x1p-1074);
    float acc = 0;
    for (int i = 1; i <= 100; i++) acc += 1.0f / i;
    double dacc = 0;
    for (int i = 1; i <= 100; i++) dacc += 1.0 / i;
    printf("%.9g %.17g\n", acc, dacc);
    double x = 2.5;
    x += 1; x *= 3; x /= 4; x -= 0.125;
    printf("%.17g %d %d\n", x, x > 2, 2.0f == 2.0);
    printf("%d %d\n", 0.1 + 0.2 == 0.3, 0.1f + 0.2f == 0.3f);
    return 0;
}
