#include <pthread.h>
#include <stdio.h>
#include <stdatomic.h>
struct __attribute__((aligned(8))) Pair { unsigned a, b; };
extern _Atomic double total;
extern _Atomic float small;
extern struct Pair pair;
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
    printf("%.0f %.0f %u %u\n", (double)total, (double)small, out.a, out.b);
    return total != 80000.0 || small != 80000.0f || out.a != 80000 || out.b != 80000;
}
