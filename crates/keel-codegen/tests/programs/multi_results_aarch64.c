#include <stdio.h>

struct hfa4 { float a, b, c, d; };
struct hfa3 { double a, b, c; };

struct hfa4 k_hfa4(float, float, float, float);
double k_call_hfa3(double);

struct hfa3 c_hfa3(double a, double b, double c) { return (struct hfa3){c, a, b}; }

int main(void) {
    int failures = 0;
    struct hfa4 r = k_hfa4(1.5f, 2.5f, 3.5f, 4.5f);
    failures += r.a != 2.5f || r.b != 3.5f || r.c != 4.5f || r.d != 1.5f;
    /* c_hfa3(x + 1, x, 1) is {1, x + 1, x}. */
    for (double x = -3; x < 4; x += 1.25)
        failures += k_call_hfa3(x) != 1 + 4 * (x + 1) + 16 * x + x;
    if (failures) printf("%d failures\n", failures);
    return failures != 0;
}
