void *read_return(void) {
    return __builtin_return_address(0);
}

static __attribute__((always_inline)) inline void *inline_return(void) {
    return __builtin_extract_return_addr(__builtin_return_address(0));
}

void *read_inlined_return(void) {
    asm("");
    return inline_return();
}

extern void clobber_callers(void);
void *read_return_after_call(void) {
    clobber_callers();
    return __builtin_return_address(0);
}

void *read_aligned_return(void) {
    volatile unsigned char pad[64] __attribute__((aligned(64)));
    pad[0] = 17;
    return __builtin_return_address(0);
}

volatile unsigned long return_extent = 96;
extern void touch_return_buffer(void *);
void *read_dynamic_return(void) {
    unsigned char *pad = __builtin_alloca(return_extent);
    pad[0] = 29;
    touch_return_buffer(pad);
    return __builtin_return_address(0);
}

int return_address_evaluations;
static __attribute__((noinline)) void *observed_pointer(void *p) {
    ++return_address_evaluations;
    return p;
}
void *return_identity_once(void *p) {
    return __builtin_extract_return_addr(__builtin_frob_return_addr(observed_pointer(p)));
}
