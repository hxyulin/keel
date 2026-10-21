typedef struct { long x, y; } Pair;
static Pair *captured;
static __attribute__((noinline)) void mutate_captured(void) { captured->y += 5; }
static __attribute__((noinline)) long snapshot(Pair a, Pair b, Pair *alias) {
    captured = &a;
    a.x += 7;
    alias->x += 100;
    mutate_captured();
    return a.x + 10*b.x + a.y + b.y;
}
static __attribute__((noinline)) long pressure(Pair a, Pair b, Pair c, Pair d, Pair e,
                                             Pair f, Pair g, Pair h, Pair i, Pair j) {
    return a.x+a.y+b.x+b.y+c.x+c.y+d.x+d.y+e.x+e.y+f.x+f.y+g.x+g.y+h.x+h.y+i.x+i.y+j.x+j.y;
}
static __attribute__((noinline)) Pair result(Pair a) { a.x += 9; return a; }
static __attribute__((noinline)) long addressed(Pair a) { return a.x+a.y; }
long (*callback)(Pair) = addressed;
long public_pair(Pair a) { return a.x+a.y; }
long aggregate_transport(long x) {
    Pair p = {x,x+3};
    long r = snapshot(p,p,&p);
    r += p.x;
    Pair q = {x,x+3};
    r += pressure(q,q,q,q,q,q,q,q,q,q);
    Pair z = result(q);
    return r + z.x + z.y + callback(q);
}
