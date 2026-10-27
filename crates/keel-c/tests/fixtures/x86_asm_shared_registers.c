/* These templates model shared constraints without privileged instructions. */
unsigned long shared_fixed(unsigned long x, unsigned long live) {
    unsigned long out;
    asm("addq $17,%0" : "=a"(out) : "a"(x) : "cc");
    return out ^ x ^ live;
}
unsigned long shared_readwrite(unsigned long x, unsigned long live) {
    unsigned long out = x;
    asm("addq $19,%0" : "+r"(out) : "0"(x), "0"(x) : "cc");
    return out ^ x ^ live;
}
unsigned long shared_early(unsigned long x) {
    unsigned long out;
    asm("addq $23,%0" : "=&r"(out) : "0"(x), "0"(x) : "cc");
    return out ^ x;
}
unsigned long shared_mixed(unsigned long x, unsigned long live) {
    unsigned long out;
    /* A narrow last input must not replace the full-width initializer. */
    asm("addq $29,%0" : "=a"(out) : "0"(x), "a"((unsigned short)x) : "cc");
    return out ^ x ^ live;
}
unsigned long shared_reverse(unsigned long x) {
    unsigned long out;
    asm("addq $31,%0" : "=r"(out) : "0"((unsigned char)x), "0"(x) : "cc");
    return out ^ x;
}
unsigned shared_status(unsigned mask) {
    unsigned status;
    /* XSAVES-style status output and mask input share EAX. */
    asm("xorl $0x1234,%0" : "=a"(status) : "a"(mask) : "cc");
    return status ^ mask;
}
unsigned shared_narrow(unsigned long x) {
    unsigned out;
    asm("xorq $0x1234,%q0" : "=a"(out) : "a"(x) : "cc");
    return out ^ (unsigned)x;
}
unsigned long shared_input_only(unsigned long x) {
    asm volatile("" : : "a"(x), "a"((unsigned char)x));
    return x;
}
unsigned long shared_mul(unsigned long a, unsigned long b, unsigned long d) {
    unsigned long out;
    asm("mulq %2; divq %3" : "=a"(out) : "a"(a), "r"(b), "r"(d) : "rdx", "cc");
    return out ^ a ^ b ^ d;
}
unsigned long shared_constants(unsigned long live) {
    unsigned long out;
    asm("addq $37,%0" : "=a"(out) : "0"(0x1234567887654321UL), "a"(0x1234567887654321UL) : "cc");
    return out ^ live;
}
unsigned long shared_low_constant(void) {
    unsigned long out;
    asm("addq $41,%0" : "=a"(out) : "a"(0xfedcba98765432ffUL), "0"((unsigned char)255) : "cc");
    return out;
}
unsigned long shared_pointer(void *p) {
    unsigned long out;
    asm("addq $43,%0" : "=a"(out) : "a"(p), "0"((unsigned long)p) : "cc");
    return out ^ (unsigned long)p;
}
int guest_main(void) {
    unsigned long values[] = {0, 1, 0x80, 0x8000, 0x80000000UL, 0xfedcba9876543210UL};
    unsigned long live = 0xabcdef9876543210UL;
    for (unsigned i = 0; i < 6; ++i) {
        unsigned long x = values[i];
        if (shared_fixed(x,live) != ((x+17)^x^live)) return 1;
        if (shared_readwrite(x,live) != ((x+19)^x^live)) return 2;
        if (shared_early(x) != ((x+23)^x)) return 3;
        if (shared_mixed(x,live) != ((x+29)^x^live)) return 4;
        if (shared_reverse(x) != ((x+31)^x)) return 5;
        if (shared_status((unsigned)x) != 0x1234) return 6;
        if (shared_narrow(x) != 0x1234) return 11;
        if (shared_input_only(x) != x) return 12;
        if (shared_constants(x) != ((0x1234567887654321UL+37)^x)) return 7;
        if (shared_pointer((void *)x) != ((x+43)^x)) return 8;
    }
    if (shared_low_constant() != 0xfedcba9876543328UL) return 9;
    if (shared_mul(1234567, 456789, 71) != ((1234567UL*456789/71)^1234567^456789^71)) return 10;
    return 0;
}
