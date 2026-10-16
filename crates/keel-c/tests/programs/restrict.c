#include <stdio.h>

__attribute__((noinline)) int separate(int *restrict p, int *restrict q) {
    int before = *p;
    *q = 9;
    return before + *p;
}
__attribute__((noinline)) int readonly(const int *restrict p, const int *restrict q) {
    return *p + *q;
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
int main(void) {
    int a = 4, b = 5;
    printf("%d\n", separate(&a, &b));
    printf("%d %d\n", a, b);
    /* Equal restricted pointer values are legal when only read. */
    printf("%d\n", readonly(&a, &a));
    /* Modified aliases are tested only without restrict. */
    printf("%d\n", ordinary(&a, &a));
    printf("%d\n", observed(&a, &b));
    return 0;
}
