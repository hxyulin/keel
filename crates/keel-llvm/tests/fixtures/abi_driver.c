#include <stdio.h>
#include <stdint.h>
signed char ext_s8(signed char x){return (signed char)(x^0x5a);}
unsigned char ext_u8(unsigned char x){return (unsigned char)(x+17);}
short ext_s16(short x){return (short)(x^0x5a5a);}
unsigned short ext_u16(unsigned short x){return (unsigned short)(x+2357);}
unsigned long ext_stack(unsigned long a,unsigned long b,unsigned long c,unsigned long d,unsigned long e,unsigned long f,unsigned long g,unsigned long h,unsigned long i,unsigned long j,void *p,double x0,double x1,double x2,double x3,double x4,double x5,double x6,double x7,double x8,double x9) {
    return a+2*b+3*c+4*d+5*e+6*f+7*g+8*h+9*i+10*j+*(unsigned long*)p+(unsigned long)(x0+x1+x2+x3+x4+x5+x6+x7+x8+x9);
}
extern long abi_small(long);
extern unsigned long abi_stack(unsigned long,void*);
extern signed char own_s8(signed char);
extern unsigned char own_u8(unsigned char);
extern short own_s16(short);
extern unsigned short own_u16(unsigned short);
int main(void) {
    unsigned long value=73;
    for(long x=-32769;x<=32769;x+=127) {
        printf("%ld %lu %d %u %d %u\n",abi_small(x),abi_stack((unsigned long)x,&value),
            own_s8((signed char)x),own_u8((unsigned char)x),own_s16((short)x),own_u16((unsigned short)x));
    }
    return 0;
}
