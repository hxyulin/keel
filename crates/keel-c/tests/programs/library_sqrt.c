#include <errno.h>
#include <math.h>
#include <stdio.h>
#include <string.h>

static void show(double r) {
    unsigned long long bits;
    memcpy(&bits, &r, sizeof bits);
    if (isnan(r))
        printf(" nan");
    else
        printf(" %016llx", bits);
}

int main(void) {
    static const double inputs[] = {4.0, 2.0, 0.0, -0.0, -1.0, -INFINITY, INFINITY, NAN, -NAN, 1e-310};
    for (unsigned i = 0; i < sizeof inputs / sizeof inputs[0]; i++) {
        volatile double x = inputs[i];
        show(sqrt(x));
        show(sqrtf((float)x));
#ifdef __APPLE__
        show(sqrtl(x));
#endif
        printf("\n");
    }
    volatile int nine = 9;
    show(sqrt(nine));
    volatile double negative = -1.0, positive = 4.0;
    errno = 0;
    show(sqrt(positive));
    printf(" %d", errno);
    show(sqrt(negative));
    printf(" %s", errno == EDOM ? "EDOM" : errno ? "other" : "0");
    errno = 0;
    show(sqrtf((float)negative));
    printf(" %s\n", errno == EDOM ? "EDOM" : errno ? "other" : "0");
    return 0;
}
