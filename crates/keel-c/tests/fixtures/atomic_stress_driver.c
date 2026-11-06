#include <pthread.h>
#include <stdio.h>
#include <stdatomic.h>
struct __attribute__((aligned(8))) Pair { unsigned a, b; };
extern _Atomic double total;
extern _Atomic float small;
extern struct Pair pair;
struct __attribute__((aligned(16))) Wide { unsigned long a, b; };
extern struct Wide wide;
extern void *keel_worker(void *);
static void *clang_worker(void *unused) {
    for (int i = 0; i < 20000; ++i) {
        total += 1.0;
        ++small;
        struct Pair expected, desired;
        __atomic_load(&pair, &expected, __ATOMIC_ACQUIRE);
        do {
            if (expected.a != expected.b) return (void *)1;
            desired.a = expected.a + 1;
            desired.b = expected.b + 1;
        } while (!__atomic_compare_exchange(&pair, &expected, &desired, 1, __ATOMIC_ACQ_REL, __ATOMIC_ACQUIRE));
        struct Wide seen, next;
        __atomic_load(&wide, &seen, __ATOMIC_ACQUIRE);
        do {
            if (seen.a != seen.b) return (void *)1;
            next.a = seen.a + 1;
            next.b = seen.b + 1;
        } while (!__atomic_compare_exchange(&wide, &seen, &next, 1, __ATOMIC_ACQ_REL, __ATOMIC_ACQUIRE));
    }
    return unused;
}
int main(void) {
    pthread_t threads[4];
    for (int i = 0; i < 4; ++i)
        if (pthread_create(&threads[i], 0, i & 1 ? clang_worker : keel_worker, 0)) return 1;
    for (int i = 0; i < 4; ++i) {
        void *result;
        if (pthread_join(threads[i], &result) || result) return 2;
    }
    struct Pair out;
    __atomic_load(&pair, &out, __ATOMIC_SEQ_CST);
    struct Wide w;
    __atomic_load(&wide, &w, __ATOMIC_SEQ_CST);
    printf("%.0f %.0f %u %u %lu %lu\n", (double)total, (double)small, out.a, out.b, w.a, w.b);
    return total != 80000.0 || small != 80000.0f || out.a != 80000 || out.b != 80000 ||
           w.a != 80000 || w.b != 80000;
}
