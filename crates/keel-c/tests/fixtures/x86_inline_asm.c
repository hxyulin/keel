#ifndef NAME
#define NAME(x) x
#endif
unsigned long NAME(asm_add)(unsigned long a, unsigned long b) {
    asm volatile("addq %[b],%[a]" : [a] "+r"(a) : [b] "r"(b) : "cc");
    return a;
}
unsigned long NAME(asm_fixed)(unsigned long a, unsigned long b) {
    asm("xchgq %0,%1; addq $7,%0" : "+a"(a), "+d"(b) :: "cc");
    return a ^ b;
}
unsigned long NAME(asm_six)(unsigned long a, unsigned long b, unsigned long c,
    unsigned long d, unsigned long s, unsigned long t) {
    asm("addq %1,%0; xorq %2,%0; addq %3,%0; xorq %4,%0; addq %5,%0"
        : "+a"(a) : "b"(b), "c"(c), "d"(d), "S"(s), "D"(t) : "cc");
    return a;
}
unsigned long NAME(asm_mul)(unsigned long a, unsigned long b) {
    unsigned long lo, hi;
    asm("mulq %3" : "=&a"(lo), "=&d"(hi) : "0"(a), "r"(b) : "cc");
    return lo ^ hi;
}
unsigned long NAME(asm_early)(unsigned long a, unsigned long b) {
    unsigned long out;
    asm("movq %1,%0; addq %2,%0" : "=&r"(out) : "r"(a), "r"(b) : "cc");
    return out ^ a ^ b;
}
unsigned long NAME(asm_views)(unsigned long a) {
    asm("movb $0x12,%b0; xorw $0x3400,%w0; addl $0x560000,%k0; xorq $0x78000000,%q0"
        : "+r"(a) :: "cc");
    return a;
}
unsigned long NAME(asm_constants)(unsigned long a) {
    asm("addq %1,%0; xorq $%c2,%0; addq $%c[n],%0"
        : "+r"(a) : "i"(7), "n"(0x1234), [n] "i"(3) : "cc");
    return a;
}
unsigned long NAME(asm_clobber)(unsigned long a, unsigned long b, unsigned long c,
    unsigned long d, unsigned long e, unsigned long f, unsigned long g,
    unsigned long h) {
    unsigned long out;
    asm("movq %1,%0; addq $9,%0; movq $1,%%rbx; movq $2,%%r12; movq $3,%%r13; movq $4,%%r14; movq $5,%%r15"
        : "=&r"(out) : "r"(a) : "rbx", "r12", "r13", "r14", "r15", "cc", "memory");
    return out ^ (a + 3*b + 5*c + 7*d + 11*e + 13*f + 17*g + 19*h);
}
unsigned long NAME(asm_unique)(unsigned long a) {
    asm(".Lkeel_unique_%=: addq $1,%0" : "+r"(a) :: "cc");
    asm(".Lkeel_unique_%=: addq $2,%0" : "+r"(a) :: "cc");
    return a;
}
unsigned long NAME(asm_loop)(unsigned long a) {
    unsigned long count = 3;
    asm("1: addq $7,%0; subq $1,%1; jnz 1b" : "+r"(a), "+r"(count) :: "cc");
    return a;
}
unsigned long NAME(asm_once)(unsigned long *p, unsigned long *q) {
    unsigned long *begin = p;
    asm("addq %1,%0" : "+r"(*p++) : "r"((*q)++) : "cc");
    return *begin + (unsigned long)(p - begin) + *q;
}
unsigned char NAME(asm_byte)(unsigned char a, unsigned char b) {
    asm("addb %1,%0" : "+r"(a) : "r"(b) : "cc");
    return a;
}
unsigned short NAME(asm_word)(unsigned short a, unsigned short b) {
    asm("xorw %1,%0" : "+r"(a) : "r"(b) : "cc");
    return a;
}
unsigned int NAME(asm_dword)(unsigned int a, unsigned int b) {
    asm("addl %1,%0" : "+r"(a) : "r"(b) : "cc");
    return a;
}
unsigned long *NAME(asm_pointer)(unsigned long *p) {
    asm("addq $8,%0" : "+r"(p) :: "cc");
    return p;
}
unsigned long NAME(asm_unused)(unsigned long a) {
    unsigned long unused;
    asm("movq %1,%0" : "=r"(unused) : "r"(a));
    asm volatile("" ::: "memory", "cc", "eax", "r11", "ah", "ch", "dh", "bh");
    asm("nop");
    return a + 1;
}
unsigned long NAME(asm_remat)(unsigned long a) {
    unsigned long x = 0x123456789abcdef0UL;
    unsigned long y = 0x6edcba9876543210UL;
    asm("xorq %1,%0; addq %2,%0" : "+c"(a) : "a"(x), "d"(y) : "cc");
    return a;
}
unsigned long NAME(asm_memory_barrier)(unsigned long *p) {
    unsigned long before = *p;
    asm volatile("addq $1,(%0)" :: "D"(p) : "memory", "cc");
    return before ^ *p;
}

unsigned long NAME(asm_aligned)(unsigned long a) {
    _Alignas(32) unsigned long words[4] = {a, a ^ 3, a + 7, a ^ 11};
    asm("addq $7,(%1); xorq $3,%0" : "+r"(a) : "r"(words) : "memory", "cc");
    return a ^ words[0] ^ words[1] ^ words[2] ^ words[3];
}

unsigned long NAME(asm_scratch)(unsigned long a, unsigned long b) {
    unsigned long out;
    asm("movq %1,%0; movq $11,%%rax; movq $12,%%rcx; movq $13,%%rdx; movq $14,%%r10; movq $15,%%r11"
        : "=&S"(out) : "D"(a) : "rax", "rcx", "rdx", "r10", "r11");
    return out ^ b;
}
unsigned long NAME(asm_flags)(unsigned long a, unsigned long b) {
    int less = a < b;
    asm("cmpq $0,%0" :: "r"(a) : "cc");
    return less ? a : b;
}
unsigned long NAME(asm_macro)(unsigned long a) {
    asm(".macro keel_inc operand\n addq $5,\\operand\n.endm" ::: "memory");
    asm("keel_inc %0\n.purgem keel_inc" : "+r"(a) :: "cc");
    return a;
}

extern unsigned char asm_deferred_anchor[64];
static __attribute__((always_inline)) inline void *deferred_rip(void *p) {
    asm("leaq %c1(%%rip),%0" : "=r"(p) : "i"(p));
    return p;
}
static __attribute__((always_inline)) inline void *deferred_outer(void *p) {
    return deferred_rip(p);
}
static __attribute__((always_inline)) inline unsigned long deferred_add(unsigned long x, unsigned long n) {
    asm(".Ldeferred_%=: addq %1,%0" : "+r"(x) : "n"(n) : "cc");
    return x;
}
unsigned long NAME(asm_deferred_number)(unsigned long x) {
    return deferred_add(deferred_add(x, 17 + 6), 42);
}
unsigned long NAME(asm_deferred_address)(unsigned long x) {
    unsigned char *a = deferred_outer(asm_deferred_anchor + 7);
    unsigned char *b = deferred_rip(asm_deferred_anchor + 19);
    return (x ^ (unsigned long)a) + (unsigned long)b;
}
