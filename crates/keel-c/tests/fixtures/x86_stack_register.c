register unsigned long current_stack_pointer asm("rsp");
register unsigned long current_stack_pointer asm("rsp");

static __attribute__((always_inline)) inline unsigned long sample_rsp(void) {
    return current_stack_pointer;
}

unsigned long read_rsp_direct(void) {
    return current_stack_pointer;
}

int ordinary_register_local(register int x) {
    register int next = x + 1;
    return next;
}

unsigned long stack_register_snapshot(unsigned long size,
    unsigned long a, unsigned long b, unsigned long c, unsigned long d,
    unsigned long e, unsigned long f, unsigned long g, unsigned long *out) {
    unsigned long before = sample_rsp();
    unsigned long observer_before;
    asm volatile("movq %%rsp,%0" : "=r"(observer_before));
    volatile unsigned char *buffer = __builtin_alloca(size);
    buffer[0] = 13;
    buffer[size - 1] = 29;
    unsigned long after = current_stack_pointer;
    unsigned long observer_after;
    asm volatile("movq %%rsp,%0" : "=r"(observer_after));
    out[0] = before;
    out[1] = observer_before;
    out[2] = after;
    out[3] = observer_after;
    out[4] = buffer[0] + buffer[size - 1];
    out[5] = (a ^ (b + 3)) + (c ^ (d + 5)) + (e ^ (f + 7)) + g;
    return before - after;
}
