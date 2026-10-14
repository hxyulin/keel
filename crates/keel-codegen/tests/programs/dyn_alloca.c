#include <stdint.h>
#include <stdio.h>

int64_t k_vla(int64_t n);
int64_t k_loop(int64_t n);
int64_t k_outer(int64_t n);

static int failures;
static int64_t *filled;
static int64_t filled_n;

void c_fill(int64_t *p, int64_t n) {
    /* 16-byte aligned, like any stack memory. */
    if ((uintptr_t)p % 16) {
        printf("stack_alloc memory %p is not 16-byte aligned\n", (void *)p);
        failures++;
    }
    for (int64_t i = 0; i < n; i++) p[i] = i;
    filled = p;
    filled_n = n;
}

/* Called after the allocation with stack arguments; the allocated memory
   must still hold what c_fill wrote. */
int64_t c_many(int64_t *p, int64_t a, int64_t b, int64_t c, int64_t d, int64_t e, int64_t f,
               int64_t g, int64_t h, int64_t i) {
    for (int64_t k = 0; k < filled_n; k++) {
        if (p[k] != k) {
            printf("allocated memory overwritten at %lld\n", (long long)k);
            failures++;
            break;
        }
    }
    return a + 2 * b + 3 * c + 4 * d + 5 * e + 6 * f + 7 * g + 8 * h + 9 * i;
}

struct big {
    int64_t a, b, c;
};

int64_t c_big(struct big s) { return s.a + 3 * s.b + 5 * s.c; }

int main(void) {
    for (int64_t n = 0; n <= 40; n += 7) {
        int64_t sum = n * (n - 1) / 2, k = 1000 * n;
        int64_t many = n + 2 * 1000 + 3 * 8 + 4 * 0 + 5 * 2 + 6 * 3 + 7 * k + 8 * n + 9 * 1000;
        int64_t big = n + 3 * k + 5 * sum;
        int64_t want = sum + many + big + k;
        int64_t got = k_vla(n);
        if (got != want) {
            printf("k_vla(%lld) = %lld, want %lld\n", (long long)n, (long long)got,
                   (long long)want);
            failures++;
        }
        if (k_outer(n) != want + 7 * n) {
            printf("k_outer(%lld) = %lld, want %lld\n", (long long)n, (long long)k_outer(n),
                   (long long)(want + 7 * n));
            failures++;
        }
    }
    int64_t n = 100000;
    if (k_loop(n) != n * (n - 1) / 2) {
        printf("k_loop(%lld) = %lld\n", (long long)n, (long long)k_loop(n));
        failures++;
    }
    /* An exit status is taken mod 256. */
    return failures != 0;
}
