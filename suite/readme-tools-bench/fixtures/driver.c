#define _POSIX_C_SOURCE 200809L
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <errno.h>
#include <limits.h>

typedef unsigned long long U;
typedef long long I;
typedef int S;
_Static_assert(sizeof(U) == 8 && sizeof(I) == 8 && sizeof(S) == 4, "integer widths");
#define CASES(X) X(positive32) X(balance32) X(positive64) X(balance64) X(sensor_mix) X(clamped) X(unsigned_high) X(adjacent) X(buckets) X(run_balance) X(widen32) X(widen_u32)
#define DECL(x) extern U rk_##x(const S *, const I *, const U *, U);
CASES(DECL)
typedef U (*Kernel)(const S *, const I *, const U *, U);
#define FN(x) rk_##x,
static Kernel functions[] = { CASES(FN) };
#define NAME(x) #x,
static const char *names[] = { CASES(NAME) };
static volatile U sink;
static U next(U *s) { *s ^= *s << 13; *s ^= *s >> 7; *s ^= *s << 17; return *s; }
static U number(const char *s) {
    char *end;
    errno = 0;
    U x = strtoull(s, &end, 10);
    if (errno || *end || !*s || *s == '-') { fprintf(stderr, "invalid integer\n"); exit(2); }
    return x;
}
static uint64_t now(void) {
    struct timespec ts;
    if (clock_gettime(CLOCK_MONOTONIC, &ts)) { perror("clock_gettime"); exit(2); }
    return (uint64_t)ts.tv_sec * 1000000000ULL + (uint64_t)ts.tv_nsec;
}
int main(int argc, char **argv) {
    if (argc != 7 || (strcmp(argv[1], "check") && strcmp(argv[1], "time"))) {
        fprintf(stderr, "usage: driver check|time CASE LENGTH REPS SEED EXPECTED\n"); return 2;
    }
    unsigned k = 0;
    while (k < sizeof(names)/sizeof(*names) && strcmp(argv[2], names[k])) ++k;
    U n = number(argv[3]), reps = number(argv[4]), seed = number(argv[5]), expected = number(argv[6]);
    if (k == sizeof(names)/sizeof(*names) || n > 16777216 || !reps || !seed) return 2;
    S *a = calloc(n + 1, sizeof(*a));
    I *b = calloc(n + 1, sizeof(*b));
    U *u = calloc(n + 1, sizeof(*u));
    if (!a || !b || !u) return 2;
    static const S edges32[] = {INT_MIN, INT_MAX, -129, -128, -1, 0, 1, 127, 128};
    static const I edges64[] = {LLONG_MIN, LLONG_MAX, -1, 0, 1};
    static const U edgesU[] = {0, 1, 9223372036854775807ULL, 9223372036854775808ULL, ULLONG_MAX};
    U state = seed;
    for (U i = 0; i < n; ++i) {
        U r = next(&state);
        S mag32 = (S)(r & 2147483647ULL);
        a[i] = (r & 2147483648ULL) ? -mag32 - 1 : mag32;
        r = next(&state);
        I mag64 = (I)(r & 9223372036854775807ULL);
        b[i] = (r >> 63) ? -mag64 - 1 : mag64;
        u[i] = next(&state);
        if (i % 3 == 0) {
            a[i] = edges32[(i / 3) % 9]; b[i] = edges64[(i / 3) % 5]; u[i] = edgesU[(i / 3) % 5];
        }
    }
    U actual = functions[k](a, b, u, n);
    if (actual != expected) {
        fprintf(stderr, "oracle mismatch %s: %llu != %llu\n", names[k], actual, expected); return 1;
    }
    /* Separate object, no LTO: every invocation remains observable to compiler. */
    U total = 0;
    uint64_t start = now();
    if (!strcmp(argv[1], "time")) {
        for (U r = 0; r < reps; ++r) { U value = functions[k](a, b, u, n); sink = value; total += value; }
    } else { total = actual; reps = 1; }
    uint64_t elapsed = now() - start;
    if (total != expected * reps) return 1;
    printf("{\"case\":\"%s\",\"length\":%llu,\"reps\":%llu,\"seed\":%llu,\"result\":%llu,\"checksum\":%llu,\"elapsed_ns\":%llu}\n",
           names[k], n, reps, seed, actual, total, (U)elapsed);
    free(a); free(b); free(u);
    return 0;
}
