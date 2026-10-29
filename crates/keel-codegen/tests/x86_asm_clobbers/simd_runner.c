#include <stdint.h>
#include <string.h>
#define DECL(name) extern void name(const void*,void*,void*);
DECL(simd_f32) DECL(simd_f64) DECL(simd_64) DECL(simd_128)
DECL(simd_256) DECL(simd_barrier) DECL(simd_clobber) DECL(simd_mixed)
int main(void) {
    void (*run[])(const void*,void*,void*)={simd_64,simd_128,simd_256,simd_barrier,simd_clobber};
    for(unsigned f=0;f<5;f++)for(unsigned p=0;p<32;p++) {
        uint32_t x[8]; unsigned char out[34],live[34];
        for(unsigned i=0;i<8;i++)x[i]=(uint32_t)(0x9e3779b9u*(p+1)+0x12345678u*i);
        memset(out,0xa5,sizeof out);memset(live,0xa5,sizeof live);
        run[f](x,out+1,live+1);
        unsigned lanes=f==0?2:(f==1||f==3?4:8);
        for(unsigned i=0;i<lanes;i++) {
            uint32_t a,b;memcpy(&a,out+1+4*i,4);memcpy(&b,live+1+4*i,4);
            if(a!=(f>=3?x[i]:x[i]+x[i])||b!=x[i])return 1;
        }
        if(out[0]!=0xa5||live[0]!=0xa5)return 2;
        for(unsigned i=1+4*lanes;i<sizeof out;i++)if(out[i]!=0xa5||live[i]!=0xa5)return 3;
    }
    for(unsigned p=0;p<32;p++) {
        float x=(float)p-10.25f,y,z;simd_f32(&x,&y,&z);if(y!=x+x||z!=x)return 4;
        double a=(double)p-10.25,b,c;simd_f64(&a,&b,&c);if(b!=a+a||c!=a)return 5;
    }
    uint32_t input[8]={0x12345678,0x9abcdef0,3,4,5,6,7,8};
    unsigned char out[58],live[58];memset(out,0xa5,58);memset(live,0xa5,58);
    simd_mixed(input,out+1,live+1);
    const unsigned offsets[]={0,8,16,48},sizes[]={4,8,32,8};
    for(unsigned i=0;i<4;i++)if(memcmp(out+1+offsets[i],input,sizes[i])||memcmp(live+1+offsets[i],input,sizes[i]))return 6;
    for(unsigned i=5;i<9;i++)if(out[i]!=0xa5||live[i]!=0xa5)return 7;
    if(out[0]!=0xa5||live[0]!=0xa5||out[57]!=0xa5||live[57]!=0xa5)return 8;
    return 0;
}
