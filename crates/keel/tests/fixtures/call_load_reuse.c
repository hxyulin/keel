/* Calls remain observable in IR; only the second successful read may vanish. */
static __attribute__((noinline)) long leaf(long *p, long x) { return x + x; }
static __attribute__((noinline)) long mutate(long *p, long x) { *p = x; return x + 1; }
/* An external definition is deliberately interposable even with a pure body. */
__attribute__((noinline)) long replaceable(long *p, long x) { return x + x; }
long reuse(long *p, long x) { long a = *p; long b = leaf(p, x); return a + b + *p; }
long mutation(long *p, long x) { long a = *p; long b = mutate(p, x); return a + b + *p; }
long indirect(long *p, long x, long (*f)(long *, long)) { long a = *p; long b = f(p, x); return a + b + *p; }
long interposable(long *p, long x) { long a = *p; long b = replaceable(p, x); return a + b + *p; }
