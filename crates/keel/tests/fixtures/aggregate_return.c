typedef struct { long x, y; } Pair;
static __attribute__((noinline)) Pair make(long x) {
    Pair r = { x, x + 7 };
    return r;
}
long aggregate_result(long x) {
    Pair r = make(x);
    return r.x * 3 + r.y;
}
static __attribute__((noinline)) long consume(Pair a, long ignored) {
    return a.x * 11 + a.y + ignored;
}
long aggregate_snapshot(long x) {
    Pair r = make(x);
    Pair frozen = r;
    r.x = x + 20;
    return consume(frozen, r.x);
}
static Pair *captured;
static __attribute__((noinline)) long observe(Pair *p) {
    captured = p;
    return p->x;
}
long aggregate_capture(long x) {
    Pair r = make(x);
    long a = observe(&r);
    captured->y += 3;
    return a + r.y;
}
typedef struct { long x, y, z; } Triple;
static __attribute__((noinline)) Triple make_large(long x) {
    Triple r = { x, x + 1, x + 2 };
    return r;
}
static __attribute__((noinline)) long consume_large(Triple r) {
    return r.x + 2 * r.y + 3 * r.z;
}
long aggregate_large(long x) {
    Triple r = make_large(x);
    return consume_large(r);
}
