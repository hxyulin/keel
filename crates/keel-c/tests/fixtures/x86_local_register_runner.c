extern unsigned long input_binding(unsigned long), output_binding(unsigned long), readwrite_binding(unsigned long), tied_binding(unsigned long), named_binding(unsigned long), early_binding(unsigned long,unsigned long), pointer_binding(unsigned long *), ordinary_binding(unsigned long), scope_binding(unsigned long), volatile_binding(unsigned long), call_binding(unsigned long,unsigned long *);
extern int narrow_binding(int);
unsigned long opaque_transform(unsigned long x) { return (x ^ 0x87654321UL) + 17; }
asm(".text\n.globl bound_call_bridge\n.type bound_call_bridge,@function\nbound_call_bridge:\nlea 31(%rax),%rdx\nxor $0x1234,%rax\nret\n.size bound_call_bridge,.-bound_call_bridge\n");
int main(void) {
    unsigned long values[] = {0,1,0xff,0x8000000000000000UL,0xfedcba9876543210UL};
    for (unsigned i=0; i<5; ++i) {
        unsigned long x=values[i],status;
        if (input_binding(x)!=x) return 1;
        if (output_binding(x)!=x) return 2;
        if (readwrite_binding(x)!=(x^0x1234)) return 3;
        if (tied_binding(x)!=x+17 || named_binding(x)!=x+29) return 4;
        if (early_binding(x,37)!=x+37) return 5;
        if (narrow_binding(x)!=(int)((unsigned char)x^0xa5)) return 6;
        if (pointer_binding(&x)!=x) return 7;
        if (ordinary_binding(x)!=((x+5+opaque_transform(x))^0x5678)) return 8;
        if (scope_binding(x)!=x+x+21) return 9;
        if (volatile_binding(x)!=(x^0x9876)) return 10;
#ifndef LOCAL_REGISTER_REFERENCE
        if (call_binding(x,&status)!=x+31 || status!=(x^0x1234)) return 11;
#endif
    }
    return 0;
}
