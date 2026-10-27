static __attribute__((always_inline)) inline int inline_edge(int x) {
    asm goto("testl %k0, %k0; jnz %l[hit]" :: "r"(x) : "cc" : hit);
    return 3;
hit:
    return 7;
}

unsigned long goto_pressure(int x, unsigned long a, unsigned long b,
                           unsigned long c, unsigned long d, unsigned long e,
                           unsigned long f, unsigned long g, unsigned long h) {
    int edge;
    asm goto("movq $101, %%rax; movq $102, %%rcx; movq $103, %%rdx; "
             "movq $104, %%r8; movq $105, %%r9; movq $106, %%r10; movq $107, %%r11; "
             "testl %k0, %k0; js %l1; jnz %l[positive]"
             :: "r"(x) : "rax", "rcx", "rdx", "r8", "r9", "r10", "r11", "cc", "memory"
             : negative, positive);
    edge = 11;
    goto done;
negative:
    edge = 17;
    goto done;
positive:
    edge = 23;
done:
    return (a ^ (b + 3)) + (c ^ (d + 5)) + (e ^ (f + 7)) + (g ^ (h + 11)) + edge;
}

int goto_numeric(int x, int y) {
    asm goto("cmpl %k1, %k0; jl %l2; jg %l3" :: "r"(x), "r"(y) : "cc" : less, greater);
    return 5;
less: return 13;
greater: return 19;
}

int goto_same_target(int x) {
    asm goto("testl %k0, %k0; js %l1; jnz %l2" :: "r"(x) : "cc" : hit_a, hit_b);
    return 29;
hit_a:
hit_b: return 31;
}

int goto_backwards(int n) {
    int count = 0;
again:
    count++;
    n--;
    asm goto("testl %k0, %k0; jg %l[again]" :: "r"(n) : "cc" : again);
    return count;
}

int goto_evaluate_once(int n) {
    asm goto("cmpl $3, %k0; je %l1" :: "r"(++n) : "cc" : hit);
    return n + 10;
hit: return n + 20;
}

int goto_inlined(int x) {
    return inline_edge(x) * 11 + inline_edge(x + 1);
}

/* The metadata target must enter the restoration path, even when reached by
   an indirect jump instead of an ordinary template branch. */
int goto_metadata(int x) {
    asm goto(".pushsection .rodata\n"
             ".balign 8\n.Lgoto_table_%=:\n.quad %l[hit]\n.popsection\n"
             "testl %k0, %k0; jz 1f; jmp *.Lgoto_table_%=(%%rip)\n1:"
             :: "r"(x) : "cc" : hit);
    return 37;
hit: return 41;
}

int goto_relative_metadata(int x) {
    asm goto(".pushsection .rodata\n"
             ".balign 4\n.Lgoto_relative_%=:\n.long %l[hit] - .\n.popsection\n"
             "testl %k0, %k0; jz 1f; "
             "leaq .Lgoto_relative_%=(%%rip), %%rcx; movslq (%%rcx), %%rax; addq %%rcx, %%rax; jmp *%%rax\n1:"
             :: "r"(x) : "rax", "rcx", "cc" : hit);
    return 43;
hit: return 47;
}

int goto_memory(int *p, int x) {
    int before = *p;
    asm goto("movl $17, %0; testl %k1, %k1; jnz %l2"
             :: "m"(*p), "r"(x) : "memory", "cc" : hit);
    return before + *p + 53;
hit: return before + *p + 59;
}
