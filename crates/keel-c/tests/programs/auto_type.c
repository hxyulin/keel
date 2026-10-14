/* GNU type inference: decay, aggregate copies and one-time evaluation. */
#include <stdio.h>

#define IS_TYPE(x, t) __builtin_types_compatible_p(__typeof__(x), t)
#define MAX_ONCE(a, b) ({ __auto_type left = (a); __auto_type right = (b); left > right ? left : right; })

static __auto_type global = 12UL;
static __auto_type message = "auto";
static int calls;
static int next(void) { calls++; return calls; }
static int add(int n) { return n + 7; }
struct pair { int x; double y; };

int main(void) {
    unsigned char small = 9;
    __auto_type narrow = small;
    __auto_type real = 1.25;
    __auto_type wide = 17ULL;
    printf("types %d %d %d %zu\n", IS_TYPE(narrow, unsigned char),
           IS_TYPE(real, double), IS_TYPE(wide, unsigned long long), sizeof narrow);
    printf("values %d %.2f %llu %lu %s\n", narrow, real, wide, global, message);

    int array[3] = { 10, 20, 30 };
    __auto_type pointer = array;
    __auto_type function = add;
    printf("decay %d %d %d %d\n", IS_TYPE(pointer, int *),
           IS_TYPE(function, int (*)(int)), pointer[1], function(3));

    struct pair original = { 4, 2.5 };
    __auto_type copy = original;
    copy.x = 8;
    printf("copy %d %d %d %.1f\n", IS_TYPE(copy, struct pair), original.x, copy.x, copy.y);

    __auto_type once = next();
    int maximum = MAX_ONCE(next(), next());
    printf("once %d %d %d\n", once, maximum, calls);

    const volatile __auto_type qualified = 5L;
    static __auto_type saved = 7U;
    saved++;
    printf("storage %ld %u %d\n", qualified, saved, IS_TYPE(saved, unsigned int));

    _Atomic(int) atomic = 6;
    __auto_type loaded = atomic;
    printf("atomic %d\n", loaded);

    int sum = 0;
    for (__auto_type i = 0; i < 3; i++) sum += i;
    __extension__ __auto_type extended = sum;
    printf("loop %d\n", extended);
    return 0;
}
