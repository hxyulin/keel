unsigned ptr_status(void *p) {
    unsigned status;
    asm("xorq $0x1234, %q0" : "=r"(status) : "0"(p) : "cc");
    return status;
}
void *integer_pointer(unsigned long value) {
    void *result;
    asm("addq $16, %q0" : "=r"(result) : "0"(value) : "cc");
    return result;
}
#ifndef TIES_REFERENCE
register unsigned long stack asm("rsp");
unsigned long kernel_get(unsigned long *p, int *status) {
    register unsigned long value asm("%rdx");
    int result;
    asm volatile("call mixed_tie_bridge" : "=a"(result), "=r"(value), "+r"(stack) : "0"(p) : "memory", "cc");
    *status = result;
    return value;
}
#endif
