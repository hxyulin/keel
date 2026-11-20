#define _GNU_SOURCE
#include <stdint.h>
#include <stdlib.h>
#include <unwind.h>
#include <dlfcn.h>
#define PARAMS uint64_t a,uint64_t b,uint64_t c,uint64_t d,uint64_t e,uint64_t f,uint64_t g,uint64_t h,int8_t i,uint16_t j,int32_t k,uint64_t *p,uint64_t m,uint8_t n
#define ARGS a,b,c,d,e,f,g,h,i,j,k,p,m,n
extern uint64_t incoming(PARAMS),outgoing(PARAMS),through_fallback(PARAMS),through_direct(PARAMS);
extern uint64_t indirect_stack(void*,PARAMS);
extern uint64_t indirect_large_homes(void*),large_frame(uint64_t);
uint64_t stack_probe(PARAMS) {
    return a+3*b+5*c+7*d+11*e+13*f+17*g+19*h
        +23*(uint64_t)(int64_t)i+29*j+31*(uint64_t)(int64_t)k+37*(*p)+41*m+43*n;
}
static uint64_t many_probe(uint64_t a,uint64_t b,uint64_t c,uint64_t d,uint64_t e,uint64_t f,uint64_t g,uint64_t h,uint64_t i,uint64_t j,uint64_t k,uint64_t l,uint64_t m,uint64_t n) {
    return a+3*b+5*c+7*d+11*e+13*f+17*g+19*h+23*i+29*j+31*k+37*l+41*m+43*n;
}
typedef struct { uint64_t lo,hi; } Pair;
extern uint64_t outgoing_pairs(uint64_t,uint64_t);
uint64_t pair_probe(Pair a,uint64_t b,uint64_t c,uint64_t d,uint64_t e,uint64_t f,Pair g,uint64_t h) {
    return 3*a.lo+5*a.hi+7*b+11*c+13*d+17*e+19*f+23*g.lo+29*g.hi+31*h;
}
static _Unwind_Reason_Code visit(struct _Unwind_Context *context,void *data) {
    Dl_info info;uintptr_t ip=(uintptr_t)_Unwind_GetIP(context);
    if(ip && dladdr((void*)(ip-1),&info) && info.dli_saddr==(void*)large_frame) *(unsigned*)data=1;
    return _URC_NO_REASON;
}
__attribute__((noinline)) uint64_t capture_large(uint64_t value) {
    unsigned seen=0;_Unwind_Backtrace(visit,&seen);if(!seen)abort();return value+11;
}
int main(void) {
    uint64_t value=UINT64_C(0x123456789);
    for(unsigned round=0;round<3;round++) {
        uint64_t a=1+round,b=2,c=3,d=4,e=5,f=6,g=7,h=8,m=9;
        int8_t i=round==0?-117:round==1?0:127;
        uint16_t j=round==0?65500:round==1?0:32768;
        int32_t k=round==0?-2000000000:round==1?0:INT32_MAX;
        uint8_t n=round==0?239:round==1?0:128;
        uint64_t *p=&value,expected=stack_probe(ARGS);
        if(incoming(ARGS)!=expected)return 1;
        if(outgoing(ARGS)!=stack_probe(h,g,f,e,d,c,b,a,i,j,k,p,m,n)+a)return 2;
        if(through_fallback(ARGS)!=expected+a)return 3;
        if(through_direct(ARGS)!=expected+b)return 4;
        if(indirect_stack((void*)stack_probe,ARGS)!=expected+a)return 5;
    }
    if(indirect_large_homes((void*)many_probe)!=many_probe(4187,4188,4189,4190,4191,4192,4193,4194,4195,4196,4197,4198,4199,4200))return 6;
    if(large_frame(17)!=45)return 7;
    const uint64_t inputs[]={0,1,UINT64_MAX,UINT64_C(0x8000000000000000)};
    for(unsigned i=0;i<sizeof(inputs)/sizeof(inputs[0]);i++) {
        uint64_t x=inputs[i],y=inputs[i]+11,sum=x+y,bits=x^y;
        if(outgoing_pairs(x,y)!=pair_probe((Pair){x,y},x,y,sum,bits,3,(Pair){sum,bits},5))
            return 8;
    }
    return 0;
}
