// GCC/Clang __atomic builtins (no <atomic> header): RMW, CAS loops, fences, orders.
extern "C" int printf(const char *, ...);

struct alignas(16) Pair { long a, b; };
static long shared_counter;
static int flag;
static unsigned char byte_bits;
static Pair pair;

__attribute__((noinline)) long spin(int n) {
    long seen = 0;
    for (int i = 0; i < n; i++) {
        seen += __atomic_fetch_add(&shared_counter, i, __ATOMIC_RELAXED);
        __atomic_fetch_or(&byte_bits, (unsigned char)(1u << (i & 7)), __ATOMIC_ACQ_REL);
        long expected = __atomic_load_n(&shared_counter, __ATOMIC_ACQUIRE);
        while (!__atomic_compare_exchange_n(&shared_counter, &expected, expected ^ 3, true, __ATOMIC_SEQ_CST,
                                            __ATOMIC_RELAXED)) {
        }
        __atomic_thread_fence(__ATOMIC_SEQ_CST);
    }
    int old = __atomic_exchange_n(&flag, n, __ATOMIC_RELEASE);
    __atomic_store_n(&pair.a, seen, __ATOMIC_RELEASE);
    __atomic_fetch_sub(&pair.b, 9, __ATOMIC_SEQ_CST);
    return seen + old + __atomic_fetch_and(&shared_counter, 0xffff, __ATOMIC_SEQ_CST);
}

int main(int argc, char **) {
    long r = spin(argc + 40);
    printf("atomics %ld %ld %d %ld %ld\n", r, shared_counter, (int)byte_bits, pair.a, pair.b);
    return 0;
}
