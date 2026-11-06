#include <stdint.h>
#include <stdio.h>

/* Each keel function with several results returns them as C returns a
 * struct of them, so here it is declared returning one. */
struct pair { int64_t a, b; };
struct narrow { char c; void *p; };
struct padded { int64_t a; int32_t b; };
struct sized { int32_t a; int64_t b; };
struct floats { double a, b; };

struct pair k_swap(int64_t, int64_t);
struct narrow k_narrow(int64_t, void *);
struct padded k_padded(int64_t);
struct floats k_floats(double, double);
struct pair k_twice(int64_t);
int64_t k_call(int64_t, int64_t);
int64_t k_call_sized(int32_t, int64_t);
double k_call_floats(double, double);
int64_t k_indirect(struct pair (*)(int64_t, int64_t), int64_t, int64_t);
struct pair k_forward(int64_t, int64_t);
struct pair k_forward_swapped(int64_t, int64_t);
int64_t k_fast(int64_t, double);

static int failures;

#define CHECK(got, want)                                                      \
    do {                                                                      \
        long long got_ = (got), want_ = (want);                               \
        if (got_ != want_) {                                                  \
            printf("line %d: %s = %lld, want %lld\n", __LINE__, #got, got_,   \
                   want_);                                                    \
            failures++;                                                       \
        }                                                                     \
    } while (0)

struct pair c_pair(int64_t a, int64_t b) { return (struct pair){3 * a + b, a - b}; }
struct sized c_sized(int32_t a, int64_t b) { return (struct sized){a + 1, b * 2}; }
struct floats c_floats(double a, double b) { return (struct floats){a + b, a * 2}; }

int main(void) {
    int64_t xs[] = {0, 1, -7, 123456789, -40000000000};
    for (int i = 0; i < 5; i++) {
        int64_t x = xs[i], y = 5 - 3 * x;
        struct pair p = k_swap(x, y);
        CHECK(p.a, y);
        CHECK(p.b, x);
        struct narrow n = k_narrow(x + 0x1200, &failures);
        CHECK(n.c, (char)(x + 0x1200));
        CHECK(n.p == &failures, 1);
        struct padded d = k_padded(x);
        CHECK(d.a, x + 1);
        CHECK(d.b, (int32_t)x);
        struct floats f = k_floats((double)x, 2.5);
        CHECK(f.a * 2, (int64_t)(2 * x - 5));
        CHECK(f.b * 2, (int64_t)(2 * x + 5));
        p = k_twice(x);
        CHECK(p.a, x);
        CHECK(p.b, x);

        struct pair p1 = c_pair(x, y), p2 = c_pair(p1.b, p1.a), p3 = c_pair(p2.a, x);
        CHECK(k_call(x, y), 7 * p2.a + 11 * p2.b + p3.b + x);
        CHECK(k_call_sized((int32_t)x, y), ((int64_t)(int32_t)x + 1) * 100 + 2 * y);
        CHECK(k_call_floats((double)x, 1.5), (int64_t)(3 * (x + 1.5) + 2.0 * x));
        CHECK(k_indirect(c_pair, x, y), 5 * p1.a - p1.b);
        CHECK(k_indirect(k_swap, x, y), 5 * y - x);
        p = k_forward(x, y);
        CHECK(p.a, p1.a);
        CHECK(p.b, p1.b);
        p = k_forward_swapped(x, y);
        CHECK(p.a, 3 * y + x);
        CHECK(p.b, y - x);

        int64_t small = x % 1000, ints = 0, floats = 0;
        for (int k = 0; k < 7; k++) ints += (k + 1) * (small + 7 - k);
        for (int k = 0; k < 8; k++) floats += (k + 1) * (small + 8 - k);
        CHECK(k_fast(small, (double)small), ints * 1000000 + floats);
    }
    /* An exit status is taken mod 256. */
    return failures != 0;
}
