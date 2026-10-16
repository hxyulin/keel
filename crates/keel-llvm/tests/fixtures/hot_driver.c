#include <stdint.h>
#include <stdio.h>
#include <math.h>
extern unsigned long hot(unsigned long*,unsigned long,unsigned long),bitmix(unsigned long,unsigned long);
extern int fp_compare(double,double);
extern signed char narrow_abi(signed char,unsigned char);
int main(void) {
    unsigned long a[32]; for(unsigned long i=0;i<32;i++) a[i]=~(i*2357ul);
    for(unsigned long n=0;n<=32;n++) for(unsigned long k=0;k<65;k++)
        printf("%016lx %016lx\n",hot(a,n,k),bitmix(~n,k));
    double v[]={0.,-0.,1.,-1.,NAN,INFINITY,-INFINITY};
    for(unsigned i=0;i<7;i++) for(unsigned j=0;j<7;j++) printf("%d\n",fp_compare(v[i],v[j]));
    for(int a=-128;a<128;a++) for(unsigned b=0;b<256;b+=17) printf("%d\n",narrow_abi((signed char)a,(unsigned char)b));
    return 0;
}
