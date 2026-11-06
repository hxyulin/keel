struct __attribute__((aligned(8))) Pair { unsigned a, b; };
_Atomic double total;
_Atomic float small;
struct Pair pair;
struct __attribute__((aligned(16))) Wide { unsigned long a, b; };
struct Wide wide;
void *keel_worker(void *unused) {
    for (int i = 0; i < 20000; ++i) {
        total += 1.0;
        ++small;
        struct Pair expected, desired;
        __atomic_load(&pair, &expected, 2);
        do {
            desired.a = expected.a + 1;
            desired.b = expected.b + 1;
        } while (!__atomic_compare_exchange(&pair, &expected, &desired, 1, 4, 2));
        struct Wide seen, next;
        __atomic_load(&wide, &seen, 2);
        do {
            next.a = seen.a + 1;
            next.b = seen.b + 1;
        } while (!__atomic_compare_exchange(&wide, &seen, &next, 1, 4, 2));
    }
    return unused;
}
