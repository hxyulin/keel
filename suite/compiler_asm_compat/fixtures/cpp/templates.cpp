// Templates, specialization, constexpr, lambdas. No headers: only printf.
extern "C" int printf(const char *, ...);

template <typename T, int N> struct Ring {
    T items[N];
    int head = 0, count = 0;
    void push(T v) {
        items[(head + count) % N] = v;
        if (count < N) count++; else head = (head + 1) % N;
    }
    template <typename F> T fold(T init, F f) const {
        for (int i = 0; i < count; i++) init = f(init, items[(head + i) % N]);
        return init;
    }
};

template <typename T> struct Traits { static constexpr int weight = 1; };
template <> struct Traits<double> { static constexpr int weight = 3; };
template <> struct Traits<long> { static constexpr int weight = 7; };

template <typename T> T weighted(T x) { return x * Traits<T>::weight; }

constexpr unsigned long fib(int n) { return n < 2 ? n : fib(n - 1) + fib(n - 2); }
static_assert(fib(20) == 6765, "constexpr");

template <typename... Ts> long sum_all(Ts... xs) { return (0L + ... + static_cast<long>(xs)); }

__attribute__((noinline)) long run(int seed) {
    Ring<long, 8> ring;
    for (int i = 0; i < 20; i++) ring.push(weighted<long>(seed + i));
    long folded = ring.fold(0L, [](long a, long b) { return a * 31 + b; });
    Ring<double, 4> dr;
    for (int i = 0; i < 6; i++) dr.push(weighted<double>(i * 0.5 + seed));
    double d = dr.fold(0.0, [](double a, double b) { return a + b; });
    return folded ^ static_cast<long>(d * 1000) ^ sum_all(1, 2u, 3L, 'a', (short)5) ^ (long)fib(30);
}

int main(int argc, char **) {
    long r = run(argc + 2);
    printf("templates %ld\n", r);
    return (int)(r & 0) ;
}
