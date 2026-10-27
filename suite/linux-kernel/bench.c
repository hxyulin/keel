/* Identical, reference-compiled userspace for paired disposable kernel guests. */
typedef unsigned long u64;
static long console;

static long syscall6(long n, long a, long b, long c, long d, long e, long f)
{
    register long r __asm__("rax") = n;
    register long x __asm__("r10") = d;
    register long y __asm__("r8") = e;
    register long z __asm__("r9") = f;
    __asm__ volatile("syscall" : "+a"(r)
                     : "D"(a), "S"(b), "d"(c), "r"(x), "r"(y), "r"(z)
                     : "rcx", "r11", "memory");
    return r;
}
#define sc(n, a, b, c) syscall6(n, (long)(a), (long)(b), (long)(c), 0, 0, 0)

static void text(const char *s)
{
    u64 n = 0;
    while (s[n]) ++n;
    if (sc(1, console, s, n) != (long)n)
        for (;;) __asm__ volatile("ud2");
}

static void number(u64 n)
{
    char b[24];
    unsigned i = sizeof(b);
    do { b[--i] = '0' + n % 10; n /= 10; } while (n);
    if (sc(1, console, b + i, sizeof(b) - i) != (long)(sizeof(b) - i))
        for (;;) __asm__ volatile("ud2");
}

static void finish(int success)
{
    text(success ? "KEEL_BENCH_OK\n" : "KEEL_BENCH_FAIL\n");
    __asm__ volatile("outl %0,%w1" : : "a"(success ? 0x10 : 0x11), "d"(0xf4));
    for (;;) __asm__ volatile("pause");
}

static void require(int condition) { if (!condition) finish(0); }

static u64 now(void)
{
    struct { long sec, ns; } t;
    require(sc(228, 1, &t, 0) == 0); /* CLOCK_MONOTONIC, direct syscall. */
    return (u64)t.sec * 1000000000UL + t.ns;
}

static void report(const char *name, u64 start, u64 count)
{
    u64 elapsed = now() - start;
    text("BENCH "); text(name); text(" "); number(elapsed);
    text(" "); number(count); text("\n");
}

static void child_exit(int status)
{
    sc(60, status, 0, 0);
    for (;;) __asm__ volatile("ud2");
}

static void wait_child(long child)
{
    int status = -1;
    require(syscall6(61, child, (long)&status, 0, 0, 0, 0) == child && status == 0);
}

void bench_main(void)
{
    u64 boot = now();
    console = sc(2, "/dev/console", 1, 0);
    require(console >= 0 && sc(172, 3, 0, 0) == 0);
    text("BENCH_READY "); number(boot); text("\n");

    u64 start = now();
    for (unsigned i = 0; i < 5000000; ++i)
        require(sc(39, 0, 0, 0) == 1);
    report("getpid", start, 5000000);

    start = now();
    for (unsigned i = 0; i < 300000; ++i) {
        unsigned char b[4];
        long fd = sc(2, "/init", 0, 0);
        require(fd >= 0 && sc(0, fd, b, 4) == 4 && b[0] == 127 &&
                b[1] == 'E' && b[2] == 'L' && b[3] == 'F' && sc(3, fd, 0, 0) == 0);
    }
    report("open_read_close", start, 300000);

    start = now();
    for (unsigned i = 0; i < 5120; ++i) {
        long a = syscall6(9, 0, 1048576, 3, 0x22, -1, 0);
        require((u64)a < (u64)-4095);
        volatile u64 *p = (volatile u64 *)a;
        for (unsigned page = 0; page < 256; ++page) p[page * 512] = page ^ i;
        require(sc(10, a, 1048576, 1) == 0);
        for (unsigned page = 0; page < 256; ++page) require(p[page * 512] == (page ^ i));
        require(sc(11, a, 1048576, 0) == 0);
    }
    report("page_fault_mprotect", start, 1310720);

    volatile u64 sentinel = 0x123456789abcdef0UL;
    start = now();
    for (unsigned i = 0; i < 5000; ++i) {
        long child = sc(57, 0, 0, 0);
        require(child >= 0);
        if (!child) { sentinel = i; child_exit(sentinel != i); }
        wait_child(child);
        require(sentinel == 0x123456789abcdef0UL);
    }
    report("fork_cow_wait", start, 5000);

    int to_child[2], to_parent[2];
    require(sc(22, to_child, 0, 0) == 0 && sc(22, to_parent, 0, 0) == 0);
    long child = sc(57, 0, 0, 0);
    require(child >= 0);
    if (!child) {
        for (u64 i = 0; i < 200000; ++i) {
            u64 value = 0;
            if (sc(0, to_child[0], &value, 8) != 8 || value != i ||
                sc(1, to_parent[1], &value, 8) != 8) child_exit(1);
        }
        child_exit(0);
    }
    start = now();
    for (u64 i = 0; i < 200000; ++i) {
        u64 value = -1UL;
        require(sc(1, to_child[1], &i, 8) == 8 &&
                sc(0, to_parent[0], &value, 8) == 8 && value == i);
    }
    wait_child(child);
    report("pipe_pingpong", start, 200000);
    require(sc(3, to_child[0], 0, 0) == 0 && sc(3, to_child[1], 0, 0) == 0 &&
            sc(3, to_parent[0], 0, 0) == 0 && sc(3, to_parent[1], 0, 0) == 0);
    finish(1);
}

/* Linux enters with a 16-byte aligned stack, unlike a normal function call. */
__asm__(".global _start\n_start:\nandq $-16, %rsp\ncall bench_main\nud2\n");
