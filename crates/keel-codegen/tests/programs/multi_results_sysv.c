#include <stdint.h>
#include <stdio.h>

struct mixed { int64_t a; double b; };
struct mixed_r { double a; int64_t b; };
struct narrow_float { float a; int64_t b; };
struct two_sse { double a; float b; };

struct mixed_r k_mixed(int64_t, double);
struct narrow_float k_narrow_float(float, int64_t);
struct two_sse k_two_sse(double, float);
double k_call_mixed(int64_t, double);

struct mixed c_mixed(int64_t a, double b) { return (struct mixed){a * 3, b * 2}; }

int main(void) {
    int failures = 0;
    for (int64_t x = -5; x < 6; x++) {
        double y = 0.25 * (double)x;
        struct mixed_r m = k_mixed(x, y);
        failures += m.a != y + 0.5 || m.b != x;
        struct narrow_float n = k_narrow_float((float)y, x * 1000);
        failures += n.a != (float)y || n.b != x * 1000;
        struct two_sse t = k_two_sse(y, (float)x);
        failures += t.a != y || t.b != (float)x;
        failures += k_call_mixed(x, y) != (double)(x * 3) + y * 2 + y;
    }
    if (failures) printf("%d failures\n", failures);
    return failures != 0;
}
