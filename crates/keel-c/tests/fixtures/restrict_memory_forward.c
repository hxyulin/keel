/* No system headers: both compilers receive exactly this source. */
extern int printf(const char *, ...);

__attribute__((noinline)) int separate(int *restrict p, int *restrict q) {
    int before = *p;
    *q = 9;
    return before + *p;
}
__attribute__((noinline)) int readonly(const int *restrict p, const int *restrict q) {
    return *p + *q;
}
__attribute__((noinline)) int readonly_store(const int *restrict p,
        const int *restrict q, int *restrict r) {
    int before = *p + *q;
    *r = 12;
    return before + *p;
}
__attribute__((noinline)) int ordinary(int *p, int *q) {
    int before = *p;
    *q = 9;
    return before + *p;
}
__attribute__((noinline)) int observed(volatile int *restrict p, int *restrict q) {
    int before = *p;
    *q = 11;
    return before + *p;
}
__attribute__((noinline)) void change(int *p) {
    *p += 3;
}
__attribute__((noinline)) int call_barrier(int *restrict p, int *restrict q) {
    int before = *p;
    change(p);
    *q = 13;
    return before + *p;
}
int main(void) {
    for (int i = -3; i <= 4; ++i) {
        int a = i, b = i + 20;
        int result = separate(&a, &b);
        printf("%d %d %d\n", result, a, b);
        /* Equal restricted pointers only access unmodified memory. */
        result = readonly(&a, &a);
        printf("%d %d\n", result, a);
        result = readonly_store(&a, &a, &b);
        printf("%d %d %d\n", result, a, b);
        /* Only the ordinary function modifies aliased memory. */
        result = ordinary(&a, &a);
        printf("%d %d\n", result, a);
        result = observed(&a, &b);
        printf("%d %d %d\n", result, a, b);
        result = call_barrier(&a, &b);
        printf("%d %d %d\n", result, a, b);
    }
    return 0;
}
