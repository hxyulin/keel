/* Shared by the register-allocator tests: a function that overwrites every
 * caller-saved general-purpose register (x0-x17; x18 is reserved on Apple
 * platforms), as any call is allowed to. A value the caller left in one of
 * them across the call is destroyed. */
void clobber(void) {
    __asm__ volatile(
        "mov x0, #0xdead\n\tmov x1, #0xdead\n\tmov x2, #0xdead\n\t"
        "mov x3, #0xdead\n\tmov x4, #0xdead\n\tmov x5, #0xdead\n\t"
        "mov x6, #0xdead\n\tmov x7, #0xdead\n\tmov x8, #0xdead\n\t"
        "mov x9, #0xdead\n\tmov x10, #0xdead\n\tmov x11, #0xdead\n\t"
        "mov x12, #0xdead\n\tmov x13, #0xdead\n\tmov x14, #0xdead\n\t"
        "mov x15, #0xdead\n\tmov x16, #0xdead\n\tmov x17, #0xdead"
        :
        :
        : "x0", "x1", "x2", "x3", "x4", "x5", "x6", "x7", "x8", "x9", "x10",
          "x11", "x12", "x13", "x14", "x15", "x16", "x17", "memory");
}
