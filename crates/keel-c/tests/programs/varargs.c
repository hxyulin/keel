/* Variadic functions defined in C: va_arg of every promoted type,
 * va_copy, and passing a va_list on. */
#include <stdio.h>
#include <stdarg.h>

static double sum(int n, ...) {
    va_list ap;
    va_start(ap, n);
    double total = 0;
    for (int i = 0; i < n; i++) {
        switch (va_arg(ap, int)) {
        case 'i': total += va_arg(ap, int); break;
        case 'l': total += va_arg(ap, long); break;
        case 'd': total += va_arg(ap, double); break;
        case 'p': total += *va_arg(ap, int *); break;
        }
    }
    va_end(ap);
    return total;
}

static int twice(const char *fmt, ...) {
    va_list ap, again;
    va_start(ap, fmt);
    va_copy(again, ap);
    int n = vprintf(fmt, ap);
    n += vprintf(fmt, again);
    va_end(again);
    va_end(ap);
    return n;
}

static void logf_(const char *prefix, const char *fmt, va_list ap) {
    printf("%s: ", prefix);
    vprintf(fmt, ap);
}

static void logger(const char *fmt, ...) {
    va_list ap;
    va_start(ap, fmt);
    logf_("log", fmt, ap);
    va_end(ap);
}

int main(void) {
    int x = 1000;
    char c = 'A';
    short s = -2;
    float f = 0.5f;
    printf("%g\n", sum(5, 'i', 1, 'l', 1L << 40, 'd', 0.25, 'p', &x, 'i', c));
    printf("%d\n", twice("%d %s %c %.1f|", s, "str", c, f));
    logger("%s %d %lu %p\n", "x", -1, 18446744073709551615ul, (void *)0);
    return 0;
}
