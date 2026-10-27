register unsigned long current_stack_pointer asm("rsp");
unsigned long stack_before, stack_after, stack_entry, stack_nested_entry;
unsigned long stack_probe_failed;
extern void stack_probe(void);
extern void stack_eight(void);
extern void clear_page_probe(void);
extern void clear_page_probe_alternative(void);

static __attribute__((always_inline)) inline unsigned long balanced_probe(void) {
    unsigned long snapshot = current_stack_pointer;
    asm volatile("movq %[sp], stack_before(%%rip)\n\t"
                 "call stack_probe\n\t"
                 "movq %q[sp], stack_after(%%rip)"
                 : [sp] "+r" (current_stack_pointer)
                 : : "rax", "rcx", "memory", "cc");
    return snapshot;
}

double balanced_live(unsigned long a, unsigned long b, unsigned long c,
                     unsigned long d, unsigned long e, unsigned long f,
                     unsigned long g, unsigned long h,
                     double x, double y, double z, double w,
                     unsigned long *out) {
    out[1] = balanced_probe();
    out[0] = (a ^ (b + 3)) + (c ^ (d + 5)) + (e ^ (f + 7)) + (g ^ (h + 11));
    return (x + y) * (z + w);
}

typedef double double2 __attribute__((vector_size(16)));
double balanced_simd(double2 x, double2 y) {
    balanced_probe();
    double2 sum = x + y;
    return sum[0] * sum[1];
}

unsigned long balanced_aligned(void) {
    volatile unsigned long pad[8] __attribute__((aligned(64)));
    pad[0] = 0x123456789abcdef0UL;
    pad[7] = 0xfedcba9876543210UL;
    unsigned long snapshot = balanced_probe();
    return pad[0] ^ pad[7] ^ snapshot ^ stack_after;
}

unsigned long balanced_dynamic(unsigned long n) {
    volatile unsigned char *pad = __builtin_alloca(n);
    pad[0] = 17;
    pad[n - 1] = 29;
    unsigned long snapshot = balanced_probe();
    return pad[0] + pad[n - 1] + (snapshot != stack_after);
}

unsigned long balanced_stack_arguments(unsigned long a, unsigned long b,
                                       unsigned long c, unsigned long d) {
    unsigned long result;
    asm volatile("movq %[sp], stack_before(%%rip)\n\t"
                 "subq $32, %[sp]\n\t"
                 "movq $77, (%[sp])\n\t"
                 "movq $88, 8(%[sp])\n\t"
                 "movq $55, %%r8\n\t"
                 "movq $66, %%r9\n\t"
                 "call stack_eight\n\t"
                 "addq $32, %[sp]\n\t"
                 "movq %[sp], stack_after(%%rip)"
                 : [sp] "+r" (current_stack_pointer), "=a" (result)
                 : "D" (a), "S" (b), "d" (c), "c" (d)
                 : "r8", "r9", "r10", "r11", "memory", "cc");
    return result;
}

void *balanced_clear_page(void *page) {
    void *cursor;
    /* Match Linux clear_page's =D/D fixed-register sharing and its alternate
       CALL metadata. Only the original path executes in this witness. */
    asm volatile("771: call %c[old]\n772:\n"
                 ".pushsection .altinstr_replacement, \"ax\"\n"
                 "774: call %c[new]\n775:\n.popsection\n"
                 ".pushsection .altinstructions, \"a\"\n"
                 ".long 771b-.\n.long 774b-.\n.byte 772b-771b\n.byte 775b-774b\n"
                 ".popsection\n"
                 : "+r" (current_stack_pointer), "=D" (cursor)
                 : "D" (page), [old] "i" (clear_page_probe),
                   [new] "i" (clear_page_probe_alternative)
                 : "rax", "rcx", "memory", "cc");
    return cursor;
}

unsigned long balanced_operand_alloca(unsigned long n) {
    unsigned long before = current_stack_pointer;
    unsigned long result;
    asm volatile("movq %[sp], stack_before(%%rip)\n\t"
                 "movb $17, (%[buffer])\n\tcall stack_probe\n\t"
                 "movq %[sp], stack_after(%%rip)\n\tmovq $1, %[result]"
                 : [sp] "+r" (current_stack_pointer), [result] "=r" (result)
                 : [buffer] "r" (__builtin_alloca(n))
                 : "rax", "rcx", "memory", "cc");
    return result ? before - stack_before : 0;
}

int balanced_mixed_outputs(void) {
    unsigned long memory[2] = {0, 0x7f7f7f7f7f7f7f7fUL};
    unsigned long result;
    unsigned index = 0;
    asm volatile("movq %0, stack_before(%%rip)\n\tcall stack_probe\n\t"
                 "movq %3, %[reg]\n\taddq $7, %[reg]\n\t"
                 "movq %[reg], %[mem]\n\tmovq %0, stack_after(%%rip)"
                 : "+r" (current_stack_pointer), [reg] "=r" (result),
                   [mem] "=m" (memory[index++])
                 : "r" (0x123456789abcdef0UL)
                 : "rax", "rcx", "rdx", "rsi", "rdi", "r8", "r9", "r10", "r11", "memory", "cc");
    return index == 1 && result == 0x123456789abcdef7UL
        && memory[0] == result && memory[1] == 0x7f7f7f7f7f7f7f7fUL;
}
