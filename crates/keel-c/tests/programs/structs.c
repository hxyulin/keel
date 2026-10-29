/* Structs and unions: layout, bit-fields, passing and returning by value
 * (in registers, as float aggregates, and in memory), assignment,
 * designated initializers, brace elision and compound literals. */
#include <stdio.h>
#include <string.h>
#include <stddef.h>

struct small { char c; int i; };
struct pair { long a, b; };
struct big { long v[5]; char tag; };
struct hfa { float x, y, z; };
struct dhfa { double d[2]; };
struct mixed { float f; int i; };
union num { int i; float f; unsigned char bytes[4]; };
struct bits {
    unsigned a : 3;
    signed b : 5;
    unsigned : 0;
    unsigned c : 12;
    int d : 20;
    unsigned char e : 1;
};
struct nested {
    int id;
    struct { int x, y; } pos;
    union { int as_int; float as_float; };
    char name[8];
};

struct small make_small(char c, int i) { struct small s = { c, i }; return s; }
struct pair swap(struct pair p) { struct pair q = { p.b, p.a }; return q; }
struct big grow(struct big b) {
    for (int i = 0; i < 5; i++) b.v[i] *= 2;
    b.tag++;
    return b;
}
struct hfa scale(struct hfa h, float k) { h.x *= k; h.y *= k; h.z *= k; return h; }
struct dhfa dsum(struct dhfa a, struct dhfa b) {
    return (struct dhfa){ { a.d[0] + b.d[0], a.d[1] + b.d[1] } };
}
struct mixed mix(struct mixed m) { m.f += m.i; m.i *= 2; return m; }
double many(struct hfa a, struct dhfa b, struct small c, struct big d, struct pair e,
            struct mixed f, double g, struct hfa h) {
    return a.x + a.y + a.z + b.d[0] + b.d[1] + c.c + c.i + d.v[0] + d.v[4] + d.tag + e.a +
           e.b + f.f + f.i + g + h.x + h.z;
}

int main(void) {
    printf("%zu %zu %zu %zu %zu %zu\n", sizeof(struct small), sizeof(struct big),
           sizeof(union num), sizeof(struct bits), sizeof(struct nested),
           offsetof(struct nested, name));
    struct small s = make_small('k', 77);
    printf("%c %d\n", s.c, s.i);
    struct pair p = swap((struct pair){ 1, 2 });
    printf("%ld %ld\n", p.a, p.b);
    struct big b = { { 1, 2, 3, 4, 5 }, 'a' };
    struct big b2 = grow(b);
    printf("%ld %ld %c %ld %c\n", b2.v[0], b2.v[4], b2.tag, b.v[4], b.tag);
    struct hfa h = scale((struct hfa){ 1, 2, 3 }, 1.5f);
    printf("%g %g %g\n", h.x, h.y, h.z);
    struct dhfa d = dsum((struct dhfa){ { 1.25, 2 } }, (struct dhfa){ { 3, 4.5 } });
    printf("%g %g\n", d.d[0], d.d[1]);
    struct mixed m = mix((struct mixed){ 0.5f, 3 });
    printf("%g %d\n", m.f, m.i);
    printf("%g\n", many(h, d, s, b, p, m, 0.25, h));

    union num n;
    n.f = 1.0f;
    printf("%08x %d\n", n.i, n.bytes[3]);
    struct bits bf = { 5, -3, 4000, -12345, 1 };
    printf("%u %d %u %d %u\n", bf.a, bf.b, bf.c, bf.d, bf.e);
    bf.a += 4; /* wraps in 3 bits */
    bf.b = 20; /* -12 in 5 signed bits */
    bf.d -= 1;
    bf.c = bf.c * 2;
    printf("%u %d %u %d %u\n", bf.a, bf.b, bf.c, bf.d, bf.e);
    /* Bit-fields narrower than int promote to int (C11 6.3.1.1p2). */
    struct { unsigned u:5; unsigned w:32; long l:5; long x:40; } pr = { 0 };
    printf("%d %d %d %d %d\n", (pr.u - 1) > 0, _Generic(pr.u + 0, int: 1, default: 0),
           _Generic(pr.w + 0, unsigned: 1, default: 0), _Generic(pr.l + 0, int: 1, default: 0),
           _Generic(pr.x + 0, long: 1, default: 0));
    pr.u = 1;
    printf("%d %d %d\n", (pr.u += 7) - 9 > 0, (pr.u = 3) - 4 > 0, ++pr.u - 5 > 0);
    pr.u = 5;
    pr.u /= -1;
    pr.w = 5;
    pr.w /= -1;
    printf("%u %u %d\n", pr.u, pr.w, (pr.u - 28) / 2);

    struct nested a = { .name = "hi", .pos.y = 9, 4 /* as_int */, .id = 3 };
    printf("%d %d %d %d %s\n", a.id, a.pos.x, a.pos.y, a.as_int, a.name);
    struct nested arr[3] = { [1] = { 7, { 1, 2 } }, { 8 }, };
    printf("%d %d %d %d\n", arr[0].id, arr[1].id, arr[1].pos.y, arr[2].id);
    struct nested copy = a;
    copy.name[0] = 'H';
    printf("%s %s %d\n", copy.name, a.name, memcmp(&copy, &a, sizeof a) != 0);
    int grid[3][3] = { 1, 2, 3, { 4 }, 5 };
    printf("%d %d %d %d %d\n", grid[0][2], grid[1][0], grid[1][1], grid[2][0], (int)sizeof grid);
    char str[] = "abc";
    char exact[3] = "xyz";
    printf("%zu %c%c%c\n", sizeof str, exact[0], exact[1], exact[2]);
    int *ip = (int[]){ 10, 20, 30 };
    struct small *sp = &(struct small){ 'z', 26 };
    printf("%d %c %d\n", ip[1] + ip[2], sp->c, sp->i);
    struct pair pairs[] = { 1, 2, 3, 4, 5 };
    printf("%zu %ld\n", sizeof pairs / sizeof pairs[0], pairs[2].a + pairs[2].b);
    return 0;
}
