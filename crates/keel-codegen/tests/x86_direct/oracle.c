#include <stdint.h>
extern uint64_t six(uint64_t,uint64_t,uint64_t,uint64_t,uint64_t,uint64_t);
extern uint64_t swap8(uint64_t,uint8_t);
extern uint64_t fixed(uint64_t),swap(uint64_t,uint64_t);
extern void *pick(void *,void *,uint64_t);
extern void pointer_copy(void **,void **);
extern int8_t return_s8(int8_t);
extern uint16_t return_u16(uint16_t);
extern uint64_t constant64(void);
extern uint16_t constant16(void);
extern uint32_t constant32(void);
int main(void) {
    if(constant64()!=UINT64_C(0x0123456789abcdef)||constant16()!=0xfedc||constant32()!=UINT32_C(0x80000000))return 10;
    uint64_t state=UINT64_C(0x123456789abcdef0);
    for (unsigned i=0;i<64;i++) {
        state=state*UINT64_C(6364136223846793005)+1;
        uint64_t a=state,b=state>>17,c=~state,d=state<<3,e=i+2,f=state^i;
        if(six(a,b,c,d,e,f)!=a+b+(c^d)-e*f) return 1;
        if(fixed(state)!=state) return 2;
        if(swap8(i,(uint8_t)state)!=((i&1)?243:(uint8_t)state)) return 9;
        if(swap(i,state)!=((i&1)?(uint64_t)-13:state)) return 3;
        if(return_s8((int8_t)state)!=(int8_t)state) return 4;
        if(return_u16((uint16_t)state)!=(uint16_t)state) return 5;
    }
    unsigned char bytes[24],output[24];
    for(unsigned i=0;i<24;i++){bytes[i]=(unsigned char)(i*19+3);output[i]=77;}
    if(check_widths(bytes,output)) return 6;
    if(check_comparisons()) return 7;
    void *a=&bytes[1],*b=&bytes[13],*out=0;
    pointer_copy(&a,&out);
    if(out!=a || pick(a,b,0)!=b || pick(a,b,UINT64_C(1)<<63)!=a) return 8;
    return 0;
}
