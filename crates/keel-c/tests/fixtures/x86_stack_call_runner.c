#include <stdio.h>
#include <stdint.h>
#include <string.h>

typedef double double2 __attribute__((vector_size(16)));
extern double balanced_live(unsigned long,unsigned long,unsigned long,unsigned long,
                            unsigned long,unsigned long,unsigned long,unsigned long,
                            double,double,double,double,unsigned long *);
extern double balanced_simd(double2,double2);
extern unsigned long balanced_aligned(void), balanced_dynamic(unsigned long);
extern unsigned long balanced_stack_arguments(unsigned long,unsigned long,unsigned long,unsigned long);
extern void *balanced_clear_page(void *);
extern unsigned long balanced_operand_alloca(unsigned long);
extern int balanced_mixed_outputs(void);
extern unsigned long stack_before,stack_after,stack_entry,stack_nested_entry,stack_probe_failed;
static int stack_ok(void) {
    return stack_before==stack_after && stack_entry+8==stack_before
        && (stack_entry&15)==8 && (stack_nested_entry&15)==8 && !stack_probe_failed;
}
int main(void) {
    const unsigned long a=UINT64_C(0xfedcba9876543210), b=UINT64_C(0x123456789abcdef0);
    const unsigned long c=UINT64_C(0x8000000000000001), d=UINT64_C(0xffff00000000ffff);
    const unsigned long e=UINT64_C(0x1010101010101010), f=UINT64_C(0xeeeeeeeeeeeeeeee);
    const unsigned long g=UINT64_C(0x7fffffffffffffff), h=UINT64_C(0xabcdef1234567890);
    const unsigned long want=(a^(b+3))+(c^(d+5))+(e^(f+7))+(g^(h+11));
    unsigned long out[2];
    double value=balanced_live(a,b,c,d,e,f,g,h,1.25,2.5,-3.75,8.0,out);
    if (value!=15.9375 || out[0]!=want || out[1]!=stack_before || !stack_ok()) return 1;
    if (balanced_simd((double2){1.25,2.5},(double2){4.75,-0.5})!=12.0 || !stack_ok()) return 2;
    if (balanced_aligned()!=UINT64_C(0xece8ece0ece8ece0) || !stack_ok()) return 3;
    for (unsigned long n=33;n<130;n+=17) {
        if (balanced_dynamic(n)!=46 || !stack_ok()) return 4;
    }
    if (balanced_stack_arguments(a,b,c,d)!=a+b+c+d+55+66+77+88
        || stack_before!=stack_after || stack_entry+8!=stack_before-32 || (stack_entry&15)!=8) return 5;
    struct { unsigned long before[2]; unsigned char page[4096]; unsigned long after[2]; } buffer;
    memset(&buffer,0x5a,sizeof(buffer));
    if (balanced_clear_page(buffer.page)!=buffer.page+4096) return 6;
    for (unsigned i=0;i<sizeof(buffer.page);++i) if(buffer.page[i]) return 7;
    if (buffer.before[0]!=UINT64_C(0x5a5a5a5a5a5a5a5a) || buffer.before[1]!=buffer.before[0]
        || buffer.after[0]!=buffer.before[0] || buffer.after[1]!=buffer.before[0]) return 8;
    for(unsigned long n=37;n<100;n+=19) {
        unsigned long delta=balanced_operand_alloca(n);
        if(delta<n || delta>n+128 || !stack_ok()) return 9;
    }
    if(!balanced_mixed_outputs() || !stack_ok()) return 10;
    puts("reserved-stack-call PASS");
    return 0;
}
