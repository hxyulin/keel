// thread_local with constant and dynamic initialization plus a TLS destructor, across threads.
extern "C" int printf(const char *, ...);
extern "C" int pthread_create(void **, const void *, void *(*)(void *), void *);
extern "C" int pthread_join(void *, void **);

static thread_local int counter = 5;
thread_local long accum;

static int dyn_seed = 0;
struct PerThread {
    int value;
    PerThread() : value(++dyn_seed * 100) {}
    ~PerThread() { __atomic_fetch_add(&dtors, 1, __ATOMIC_SEQ_CST); }
    static int dtors;
};
int PerThread::dtors = 0;
thread_local PerThread per_thread;

__attribute__((noinline)) long work(int n) {
    for (int i = 0; i < n; i++) {
        counter += i;
        accum += counter;
    }
    return accum + per_thread.value;
}

static void *thread_main(void *arg) {
    long r = work((int)(long)arg);
    return (void *)r;
}

int main(int argc, char **) {
    void *thread = nullptr;
    void *result = nullptr;
    long mine = work(argc + 3);
    pthread_create(&thread, nullptr, thread_main, (void *)10L);
    pthread_join(thread, &result);
    printf("tls main=%ld thread=%ld counter=%d dtors=%d\n", mine, (long)result, counter, PerThread::dtors);
    return 0;
}
