extern long reuse(long *, long), mutation(long *, long), interposable(long *, long);
extern long indirect(long *, long, long (*)(long *, long));
static long effect(long *p, long x) { *p = x - 3; return x + 7; }
int main(void) {
  for (long x = -100; x <= 100; ++x) {
    long p = x + 5;
    if (reuse(&p, x) != 4*x + 10 || p != x + 5) return 1;
    p = x + 5;
    if (mutation(&p, x) != 3*x + 6 || p != x) return 2;
    p = x + 5;
    if (indirect(&p, x, effect) != 3*x + 9 || p != x - 3) return 3;
    p = x + 5;
    #ifdef INTERPOSED
    if (interposable(&p, x) != 3*x + 9 || p != x - 9) return 4;
#else
    if (interposable(&p, x) != 4*x + 10 || p != x + 5) return 4;
#endif
  }
  return 0;
}
