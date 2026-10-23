struct __attribute__((aligned(8))) Pair { unsigned a, b; };
_Atomic double total;
_Atomic float small;
struct Pair pair;
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
    }
    return unused;
}
