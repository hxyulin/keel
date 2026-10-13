#include <stdint.h>
#include <stdio.h>
#include <string.h>

typedef struct { int32_t a, b; } S8;
typedef struct { int64_t a, b; } S16;
typedef struct { int64_t a, b, c; } S24;
typedef struct { double x, y; } D2;
typedef struct { float a, b, c; } F3;
typedef struct { float f[4]; } F4;
typedef struct { double d; int64_t l; } DL;
typedef struct { char a, b, c; } C3;
typedef struct { char c; struct { int16_t b; char s[3]; } in; } N8;
typedef struct { int64_t v[5]; } Big;

/* Hashes by field, so padding never matters. */
int64_t h_s8(const S8 *s) { return s->a * 3 + s->b * 5; }
int64_t h_s16(const S16 *s) { return s->a * 7 + s->b * 11; }
int64_t h_s24(const S24 *s) { return s->a * 13 + s->b * 17 + s->c * 19; }
int64_t h_d2(const D2 *s) { return (int64_t)(s->x * 100) * 3 + (int64_t)(s->y * 100); }
int64_t h_f3(const F3 *s) { return (int64_t)(s->a * 10) + (int64_t)(s->b * 100) + (int64_t)(s->c * 1000); }
int64_t h_f4(const F4 *s) { return (int64_t)(s->f[0] + 2 * s->f[1] + 3 * s->f[2] + 4 * s->f[3]); }
int64_t h_dl(const DL *s) { return (int64_t)(s->d * 4) + s->l * 9; }
int64_t h_c3(const C3 *s) { return s->a + s->b * 256 + s->c * 65536; }
int64_t h_n8(const N8 *s) { return s->c + s->in.b * 3 + s->in.s[0] * 5 + s->in.s[1] * 7 + s->in.s[2] * 11; }
int64_t h_big(const Big *s) {
    int64_t h = 0;
    for (int i = 0; i < 5; i++) h = h * 31 + s->v[i];
    return h;
}

/* By value, in C. */
int64_t c_s8(S8 s) { return h_s8(&s); }
int64_t c_s16(S16 s) { return h_s16(&s); }
int64_t c_s24(S24 s) { return h_s24(&s); }
int64_t c_d2(D2 s) { return h_d2(&s); }
int64_t c_f3(F3 s) { return h_f3(&s); }
int64_t c_f4(F4 s) { return h_f4(&s); }
int64_t c_dl(DL s) { return h_dl(&s); }
int64_t c_c3(C3 s) { return h_c3(&s); }
int64_t c_n8(N8 s) { return h_n8(&s); }
int64_t c_big(Big s) { return h_big(&s); }
/* Registers run out: the struct goes whole to the stack, and so does what
 * follows it. */
int64_t c_after7(int64_t a, int64_t b, int64_t c, int64_t d, int64_t e, int64_t f, int64_t g,
                 S16 s, int64_t h) {
    return a + b + c + d + e + f + g + h_s16(&s) * 2 + h * 1000;
}
int64_t c_fafter(double a, double b, double c, double d, double e, double f, double g, D2 s,
                 float h) {
    return (int64_t)(a + b + c + d + e + f + g) + h_d2(&s) * 2 + (int64_t)(h * 1000);
}
int64_t c_after8(int64_t a, int64_t b, int64_t c, int64_t d, int64_t e, int64_t f, int64_t g,
                 int64_t h, C3 x, F3 y, Big z, int32_t w) {
    return a + b + c + d + e + f + g + h + h_c3(&x) * 3 + h_f3(&y) * 5 + h_big(&z) * 7 + w;
}
/* Float aggregates on the stack are packed at their own alignment. */
int64_t c_fstack(double a, double b, double c, double d, double e, double f, double g, double h,
                 F3 x, float y, F3 z) {
    return (int64_t)(a + b + c + d + e + f + g + h) + h_f3(&x) * 3 + (int64_t)(y * 100) +
           h_f3(&z) * 7;
}
int64_t h_fstack(double a, double b, double c, double d, double e, double f, double g, double h,
                 const F3 *x, float y, const F3 *z) {
    return c_fstack(a, b, c, d, e, f, g, h, *x, y, *z);
}
int64_t h_after7(int64_t a, int64_t b, int64_t c, int64_t d, int64_t e, int64_t f, int64_t g,
                 const S16 *s, int64_t h) {
    return c_after7(a, b, c, d, e, f, g, *s, h);
}
int64_t h_fafter(double a, double b, double c, double d, double e, double f, double g,
                 const D2 *s, float h) {
    return c_fafter(a, b, c, d, e, f, g, *s, h);
}
int64_t h_after8(int64_t a, int64_t b, int64_t c, int64_t d, int64_t e, int64_t f, int64_t g,
                 int64_t h, const C3 *x, const F3 *y, const Big *z, int32_t w) {
    return c_after8(a, b, c, d, e, f, g, h, *x, *y, *z, w);
}

/* Struct results, in C. */
void fill_s16(S16 *s, int64_t x) { s->a = x; s->b = -x * 3; }
void fill_s24(S24 *s, int64_t x) { s->a = x; s->b = x + 1; s->c = x * x; }
void fill_d2(D2 *s, int64_t x) { s->x = x * 0.5; s->y = -x * 0.25; }
void fill_f3(F3 *s, int64_t x) { s->a = x * 1.5f; s->b = 2; s->c = -x; }
void fill_c3(C3 *s, int64_t x) { s->a = x; s->b = x + 1; s->c = x + 2; }
void fill_big(Big *s, int64_t x) { for (int i = 0; i < 5; i++) s->v[i] = x * i - 1; }
S16 c_make_s16(int64_t x) { S16 s; fill_s16(&s, x); return s; }
S24 c_make_s24(int64_t x) { S24 s; fill_s24(&s, x); return s; }
D2 c_make_d2(int64_t x) { D2 s; fill_d2(&s, x); return s; }
F3 c_make_f3(int64_t x) { F3 s; fill_f3(&s, x); return s; }
C3 c_make_c3(int64_t x) { C3 s; fill_c3(&s, x); return s; }

/* The keel side (structs.keel). */
int64_t k_pass_s8(const S8 *), k_pass_s16(const S16 *), k_pass_s24(const S24 *),
    k_pass_d2(const D2 *), k_pass_f3(const F3 *), k_pass_f4(const F4 *), k_pass_dl(const DL *),
    k_pass_c3(const C3 *), k_pass_n8(const N8 *), k_pass_big(const Big *);
int64_t k_s8(S8), k_s16(S16), k_s24(S24), k_d2(D2), k_f3(F3), k_f4(F4), k_dl(DL), k_c3(C3),
    k_n8(N8), k_big(Big);
int64_t k_after7(int64_t, int64_t, int64_t, int64_t, int64_t, int64_t, int64_t, S16, int64_t);
int64_t k_fafter(double, double, double, double, double, double, double, D2, float);
int64_t k_after8(int64_t, int64_t, int64_t, int64_t, int64_t, int64_t, int64_t, int64_t, C3, F3,
                 Big, int32_t);
int64_t k_fstack(double, double, double, double, double, double, double, double, F3, float, F3);
int64_t k_pass_fstack(const F3 *, const F3 *);
int64_t k_pass_after7(const S16 *), k_pass_fafter(const D2 *),
    k_pass_after8(const C3 *, const F3 *, const Big *);
int64_t k_scribble_big(Big), k_scribble_s16(S16);
int64_t k_k_s16(const S16 *), k_k_big(const Big *);
S16 k_make_s16(int64_t);
S24 k_make_s24(int64_t);
D2 k_make_d2(int64_t);
F3 k_make_f3(int64_t);
C3 k_make_c3(int64_t);
Big k_make_big(int64_t);
void k_get_s16(S16 *, int64_t), k_get_s24(S24 *, int64_t), k_get_d2(D2 *, int64_t),
    k_get_f3(F3 *, int64_t), k_get_c3(C3 *, int64_t);
int64_t k_roundtrip(int64_t);

/* Overwrites a callee's copy; the caller's struct must not change. */
void scribble(void *p, int64_t n) { memset(p, 0x5a, (size_t)n); }

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
#define SAME(a, b) CHECK(memcmp(&(a), &(b), sizeof(a)), 0)

int main(void) {
    S8 s8 = {-3, 70000};
    S16 s16 = {123456789012, -5};
    S24 s24 = {1, -2, 3};
    D2 d2 = {1.25, -7.5};
    F3 f3 = {0.5f, 2.25f, -3.0f};
    F4 f4 = {{1, 2, 3, 4}};
    DL dl = {2.5, -11};
    C3 c3 = {'x', -1, 9};
    N8 n8 = {'q', {-300, {1, 2, 3}}};
    Big big = {{1, -2, 3, -4, 5}};

    /* keel passes C's struct on to C by value */
    CHECK(k_pass_s8(&s8), h_s8(&s8));
    CHECK(k_pass_s16(&s16), h_s16(&s16));
    CHECK(k_pass_s24(&s24), h_s24(&s24));
    CHECK(k_pass_d2(&d2), h_d2(&d2));
    CHECK(k_pass_f3(&f3), h_f3(&f3));
    CHECK(k_pass_f4(&f4), h_f4(&f4));
    CHECK(k_pass_dl(&dl), h_dl(&dl));
    CHECK(k_pass_c3(&c3), h_c3(&c3));
    CHECK(k_pass_n8(&n8), h_n8(&n8));
    CHECK(k_pass_big(&big), h_big(&big));
    /* C passes by value to keel */
    CHECK(k_s8(s8), h_s8(&s8));
    CHECK(k_s16(s16), h_s16(&s16));
    CHECK(k_s24(s24), h_s24(&s24));
    CHECK(k_d2(d2), h_d2(&d2));
    CHECK(k_f3(f3), h_f3(&f3));
    CHECK(k_f4(f4), h_f4(&f4));
    CHECK(k_dl(dl), h_dl(&dl));
    CHECK(k_c3(c3), h_c3(&c3));
    CHECK(k_n8(n8), h_n8(&n8));
    CHECK(k_big(big), h_big(&big));
    /* registers run out, both ways */
    CHECK(k_after7(1, 2, 3, 4, 5, 6, 7, s16, 8), c_after7(1, 2, 3, 4, 5, 6, 7, s16, 8));
    CHECK(k_fafter(1, 2, 3, 4, 5, 6, 7, d2, 0.5f), c_fafter(1, 2, 3, 4, 5, 6, 7, d2, 0.5f));
    CHECK(k_after8(1, 2, 3, 4, 5, 6, 7, 8, c3, f3, big, -9),
          c_after8(1, 2, 3, 4, 5, 6, 7, 8, c3, f3, big, -9));
    F3 f3b = {-1.5f, 8.0f, 0.25f};
    CHECK(k_fstack(1, 2, 3, 4, 5, 6, 7, 8, f3, 0.75f, f3b), c_fstack(1, 2, 3, 4, 5, 6, 7, 8, f3, 0.75f, f3b));
    CHECK(k_pass_fstack(&f3, &f3b), c_fstack(1, 2, 3, 4, 5, 6, 7, 8, f3, 0.75f, f3b));
    CHECK(k_pass_after7(&s16), c_after7(1, 2, 3, 4, 5, 6, 7, s16, 8));
    CHECK(k_pass_fafter(&d2), c_fafter(1, 2, 3, 4, 5, 6, 7, d2, 0.5f));
    CHECK(k_pass_after8(&c3, &f3, &big), c_after8(1, 2, 3, 4, 5, 6, 7, 8, c3, f3, big, -9));
    /* the callee's copy is its own */
    Big big_before = big;
    S16 s16_before = s16;
    CHECK(k_scribble_big(big), h_big(&(Big){{0x5a5a5a5a5a5a5a5a, 0x5a5a5a5a5a5a5a5a,
                                              0x5a5a5a5a5a5a5a5a, 0x5a5a5a5a5a5a5a5a,
                                              0x5a5a5a5a5a5a5a5a}}));
    CHECK(k_scribble_s16(s16), h_s16(&(S16){0x5a5a5a5a5a5a5a5a, 0x5a5a5a5a5a5a5a5a}));
    SAME(big, big_before);
    SAME(s16, s16_before);
    CHECK(k_k_s16(&s16), h_s16(&s16));
    CHECK(k_k_big(&big), h_big(&big));
    SAME(big, big_before);

    /* keel returns structs to C */
    for (int64_t x = -2; x <= 2; x++) {
        S16 a = k_make_s16(x), a2 = c_make_s16(x);
        SAME(a, a2);
        S24 b = k_make_s24(x), b2 = c_make_s24(x);
        SAME(b, b2);
        D2 c = k_make_d2(x), c2 = c_make_d2(x);
        SAME(c, c2);
        F3 d = k_make_f3(x), d2b = c_make_f3(x);
        SAME(d, d2b);
        C3 e = k_make_c3(x), e2 = c_make_c3(x);
        SAME(e, e2);
        Big f = k_make_big(x), f2;
        fill_big(&f2, x);
        SAME(f, f2);
    }
    /* keel receives structs from C, storing exactly their bytes */
    struct { char before[8]; C3 c3; char after[5]; } guard;
    memset(&guard, '#', sizeof guard);
    k_get_c3(&guard.c3, 40);
    C3 want_c3 = c_make_c3(40);
    SAME(guard.c3, want_c3);
    for (int i = 0; i < 8; i++) CHECK(guard.before[i], '#');
    for (int i = 0; i < 5; i++) CHECK(guard.after[i], '#');
    S16 g16;
    k_get_s16(&g16, 9);
    S16 w16 = c_make_s16(9);
    SAME(g16, w16);
    S24 g24;
    k_get_s24(&g24, 9);
    S24 w24 = c_make_s24(9);
    SAME(g24, w24);
    D2 gd2;
    k_get_d2(&gd2, 9);
    D2 wd2 = c_make_d2(9);
    SAME(gd2, wd2);
    F3 gf3;
    k_get_f3(&gf3, 9);
    F3 wf3 = c_make_f3(9);
    SAME(gf3, wf3);
    /* keel to keel: make a struct, pass it on */
    CHECK(k_roundtrip(5), h_s16(&(S16){5, -15}) + h_big(&(Big){{-1, 4, 9, 14, 19}}));
    return failures;
}
