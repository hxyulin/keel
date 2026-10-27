extern unsigned ptr_status(void *);
extern void *integer_pointer(unsigned long);
extern unsigned long kernel_get(unsigned long *,int *);
asm(".text\n.globl mixed_tie_bridge\n.type mixed_tie_bridge,@function\nmixed_tie_bridge:\nmov (%rax),%rdx\nadd $31,%rdx\nmov $-7,%eax\nret\n.size mixed_tie_bridge,.-mixed_tie_bridge\n");
int base_test(void) {
    unsigned long x=0xfedcba9876543210UL;
    if (ptr_status(&x)!=(unsigned)((unsigned long)&x^0x1234)) return 1;
    if (integer_pointer((unsigned long)&x)!=(void *)((char *)&x+16)) return 2;
#ifndef TIES_REFERENCE
    int status=0;
    if (kernel_get(&x,&status)!=x+31 || status!=-7) return 3;
#endif
    return 0;
}
