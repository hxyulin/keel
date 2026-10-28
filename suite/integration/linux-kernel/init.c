/* Reference-toolchain initramfs witness. This is not compiled with Keel yet. */
static long syscall3(long number, long a, long b, long c)
{
    register long result __asm__("rax") = number;
    __asm__ volatile("syscall" : "+a"(result)
                     : "D"(a), "S"(b), "d"(c) : "rcx", "r11", "memory");
    return result;
}

void _start(void)
{
    static const char success[] = "KEEL_LINUX64_REFERENCE_OK\n";
    long console = syscall3(2, (long)"/dev/console", 1, 0);
    if (console >= 0 && syscall3(172, 3, 0, 0) == 0) {
        syscall3(1, console, (long)success, sizeof(success) - 1);
        /* Disposable QEMU fixture: isa-debug-exit maps 0x10 to host exit 33. */
        __asm__ volatile("outl %0,%w1" : : "a"(0x10), "d"(0xf4));
    }
    for (;;)
        __asm__ volatile("pause");
}
