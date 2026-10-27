unsigned long input_binding(unsigned long x) {
    register unsigned long value asm("%rdx") = x;
    unsigned long out;
    asm("movq %%rdx, %0" : "=r"(out) : "r"(value));
    return out;
}
unsigned long output_binding(unsigned long x) {
    register unsigned long value asm("r10");
    asm("movq %1, %%r10" : "=r"(value) : "r"(x));
    return value;
}
unsigned long readwrite_binding(unsigned long x) {
    register unsigned long value asm("r12") = x;
    asm("xorq $0x1234, %%r12" : "+r"(value) : : "cc");
    return value;
}
unsigned long tied_binding(unsigned long x) {
    register unsigned long input asm("r9") = x;
    unsigned long out;
    asm("addq $17, %%r9" : "=r"(out) : "0"(input) : "cc");
    return out;
}
unsigned long named_binding(unsigned long x) {
    register unsigned long out asm("r8");
    asm("addq $29, %%r8" : [result] "=r"(out) : "[result]"(x) : "cc");
    return out;
}
unsigned long early_binding(unsigned long x, unsigned long y) {
    register unsigned long out asm("r11");
    asm("movq %1, %%r11; addq %2, %%r11" : "=&r"(out) : "r"(x), "r"(y) : "cc");
    return out;
}
int narrow_binding(int x) {
    register unsigned char value asm("edx") = x;
    asm("xorb $0xa5, %%dl" : "+q"(value) : : "cc");
    return value;
}
unsigned long pointer_binding(unsigned long *p) {
    register unsigned long *value asm("rsi") = p;
    unsigned long out;
    asm("movq (%%rsi), %0" : "=r"(out) : "r"(value) : "memory");
    return out;
}
extern unsigned long opaque_transform(unsigned long);
unsigned long ordinary_binding(unsigned long x) {
    unsigned long transformed = opaque_transform(x);
    register unsigned long value asm("rax") = x + 5;
    value += transformed;
    asm("xorq $0x5678, %%rax" : "+r"(value) : : "cc");
    return value;
}
unsigned long scope_binding(unsigned long x) {
    register unsigned long value asm("r10") = x;
    { register unsigned long value asm("r9") = x + 3;
      asm("addq $7, %%r9" : "+r"(value) : : "cc"); x = value; }
    asm("addq $11, %%r10" : "+r"(value) : : "cc");
    return x + value;
}
unsigned long volatile_binding(unsigned long x) {
    register volatile unsigned long value asm("rcx") = x;
    asm("xorq $0x9876, %%rcx" : "+r"(value) : : "cc");
    return value;
}
/* The global stack dependency makes the hidden call's ABI contract explicit. */
#ifndef LOCAL_REGISTER_REFERENCE
register unsigned long current_stack_pointer asm("rsp");
unsigned long call_binding(unsigned long x, unsigned long *status) {
    register unsigned long value asm("%rdx");
    unsigned long result;
    asm volatile("call bound_call_bridge" : "=a"(result), "=r"(value), "+r"(current_stack_pointer) : "0"(x) : "memory", "cc");
    *status = result;
    return value;
}

#endif
