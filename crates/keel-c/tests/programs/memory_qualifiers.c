#include <stdio.h>

typedef volatile int VI;
typedef VI VA[2];
struct S { long x[10]; };
volatile int global_value = 7;
volatile int *get_value(void) { return &global_value; }

int main(void) {
    VI v = 3;
    int *volatile pointer = (int *)&v;
    VI *p = &v;
    __auto_type q = p;
    __typeof__(*p) copy = *q;
    *q += 2;
    VA a = {11, 13};
    struct S source = {{1,2,3,4,5,6,7,8,9,10}}, result;
    volatile struct S device = source;
    result = device;
    device = result;
    printf("%d %d %d %d %d %ld %ld\n", v, *pointer, copy, a[1], *get_value(), result.x[9], device.x[0]);
    return 0;
}
