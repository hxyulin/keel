#include <stdint.h>
#include <stddef.h>
extern uint64_t copy_bytes(void *,const void *,uint64_t,uint64_t);
extern uint64_t move_bytes(void *,const void *,uint64_t,uint64_t);
extern uint64_t fill_bytes(void *,uint8_t,uint64_t,uint64_t);
extern uint64_t roundtrip(void *,const void *,uint64_t,uint64_t);
uint64_t probe10(uint64_t a,uint64_t b,uint64_t c,uint64_t d,uint64_t e,uint64_t f,uint64_t g,uint64_t h,uint64_t i,uint64_t j) {
    return a+2*b+3*c+4*d+5*e+6*f+7*g+8*h+9*i+10*j;
}
static void initialize(unsigned char *p,unsigned seed) {for(unsigned i=0;i<160;i++)p[i]=(unsigned char)(seed+i*19);}
static void model_move(unsigned char *p,unsigned dst,unsigned src,unsigned n) {
    unsigned char temporary[64];for(unsigned i=0;i<n;i++)temporary[i]=p[src+i];for(unsigned i=0;i<n;i++)p[dst+i]=temporary[i];
}
static int different(const unsigned char *a,const unsigned char *b) {for(unsigned i=0;i<160;i++)if(a[i]!=b[i])return 1;return 0;}
int main(void) {
    const unsigned sizes[]={0,1,7,8,15,16,31,32,63};
    unsigned char a[160],b[160],source[160];
    const uint64_t tag=UINT64_C(0xfedcba9876543210);
    for(unsigned s=0;s<sizeof(sizes)/sizeof(sizes[0]);s++) {
        unsigned n=sizes[s];initialize(a,23);initialize(b,23);initialize(source,91);
        for(unsigned i=0;i<n;i++)b[3+i]=source[1+i];
        if(copy_bytes(a+3,source+1,n,tag)!=(tag^UINT64_C(0x0123456789abcdef))+25*tag+30*n||different(a,b))return 1;
        for(unsigned direction=0;direction<2;direction++) {
            initialize(a,7);initialize(b,7);unsigned dst=direction?1:4,src=direction?4:1;model_move(b,dst,src,n);
            if(move_bytes(a+dst,a+src,n,tag)!=(tag^UINT64_C(0xfedcba9876543210))+n||different(a,b))return 2;
        }
        for(unsigned byte=0;byte<3;byte++) {
            static const uint8_t fill[]={0,128,255};initialize(a,41);initialize(b,41);for(unsigned i=0;i<n;i++)b[3+i]=fill[byte];
            if(fill_bytes(a+3,fill[byte],n,tag)!=(~tag)+n||different(a,b))return 3;
        }
        initialize(a,15);initialize(b,15);b[3]=n?source[1]:0;for(unsigned i=0;i<n;i++)b[4+i]=source[1+i];
        if(roundtrip(a+3,source+1,n,tag)!=26*tag+30*n||different(a,b))return 4;
    }
    return 0;
}
