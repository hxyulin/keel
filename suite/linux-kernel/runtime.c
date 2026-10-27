/* Freestanding userspace smoke test for disposable Linux64 QEMU guests. */
static long syscall6(long number, long a, long b, long c, long d, long e, long f)
{
    register long result __asm__("rax") = number;
    register long arg4 __asm__("r10") = d;
    register long arg5 __asm__("r8") = e;
    register long arg6 __asm__("r9") = f;
    __asm__ volatile("syscall" : "+a"(result)
                     : "D"(a), "S"(b), "d"(c), "r"(arg4), "r"(arg5), "r"(arg6)
                     : "rcx", "r11", "memory");
    return result;
}
#define sc(n, a, b, c) syscall6(n, (long)(a), (long)(b), (long)(c), 0, 0, 0)

static long console;
static void message(const char *text)
{
    unsigned long size = 0;
    while (text[size])
        ++size;
    sc(1, console, text, size);
}

static void finish(int success)
{
    message(success ? "KEEL_LINUX64_RUNTIME_OK\n" : "KEEL_LINUX64_RUNTIME_FAIL\n");
    __asm__ volatile("outl %0,%w1" : : "a"(success ? 0x10 : 0x11), "d"(0xf4));
    for (;;)
        __asm__ volatile("pause");
}

void _start(void)
{
    console = sc(2, "/dev/console", 1, 0);
    if (console < 0 || sc(172, 3, 0, 0) != 0 || sc(39, 0, 0, 0) != 1)
        finish(0);

    /* VFS and initramfs: read the ELF magic from our executable. */
    unsigned char magic[4];
    long fd = sc(2, "/init", 0, 0);
    if (fd < 0 || sc(0, fd, magic, 4) != 4 || magic[0] != 127 ||
        magic[1] != 'E' || magic[2] != 'L' || magic[3] != 'F' || sc(3, fd, 0, 0))
        finish(0);
    message("KEEL_RUNTIME_VFS_OK\n");

    /* Anonymous mappings, page faults, permissions and unmapping. */
    long address = syscall6(9, 0, 65536, 3, 0x22, -1, 0);
    if ((unsigned long)address >= (unsigned long)-4095)
        finish(0);
    volatile unsigned char *memory = (volatile unsigned char *)address;
    for (unsigned long i = 0; i < 65536; ++i)
        memory[i] = (unsigned char)((i * 37) ^ (i >> 8));
    if (sc(10, address, 65536, 1))
        finish(0);
    for (unsigned long i = 0; i < 65536; ++i)
        if (memory[i] != (unsigned char)((i * 37) ^ (i >> 8)))
            finish(0);
    if (sc(10, address, 65536, 3) || sc(11, address, 65536, 0))
        finish(0);
    message("KEEL_RUNTIME_MEMORY_OK\n");

    /* Fork, COW, scheduler, pipe transport, exit status and wait. */
    int pipefd[2];
    if (sc(22, pipefd, 0, 0))
        finish(0);
    volatile unsigned long original = 0x123456789abcdef0UL;
    long child = sc(57, 0, 0, 0);
    if (child < 0)
        finish(0);
    if (!child) {
        original = 0xfedcba9876543210UL;
        unsigned long value = original;
        long written = sc(1, pipefd[1], &value, sizeof(value));
        sc(60, written == sizeof(value) ? 7 : 8, 0, 0);
        for (;;)
            __asm__ volatile("pause");
    }
    unsigned long value = 0;
    int status = 0;
    if (sc(0, pipefd[0], &value, sizeof(value)) != sizeof(value) ||
        syscall6(61, child, (long)&status, 0, 0, 0, 0) != child || status != (7 << 8) ||
        value != 0xfedcba9876543210UL || original != 0x123456789abcdef0UL)
        finish(0);
    if (sc(3, pipefd[0], 0, 0) || sc(3, pipefd[1], 0, 0))
        finish(0);
    message("KEEL_RUNTIME_PROCESS_OK\n");
    finish(1);
}
