#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <math.h>
extern uint64_t edges(uint64_t), parallel(uint8_t), loop(uint64_t);
extern uint64_t shifts(uint64_t,uint64_t), narrow(uint64_t,uint64_t);
extern uint64_t overflow(uint64_t,uint64_t);
extern uint64_t arithmetic(uint64_t,uint64_t), memory(uint64_t);
extern uint64_t nan_compare(double,double);
extern uint64_t sat_s32(double),sat_u32(double),sat_s64(double),sat_u64(double);
extern uint64_t sat_f32_s32(float),sat_f32_u32(float),sat_f32_s64(float),sat_f32_u64(float);
uint64_t helper(uint64_t x) { return x ^ UINT64_C(0x935a54d2c711e8f0); }
static void show(uint64_t x) {printf("%016llx\n",(unsigned long long)x);}
int main(void) {
    uint64_t values[]={0,1,7,8,31,32,63,64,65,127,128,255,256,UINT64_MAX,UINT64_C(0x8000000000000000)};
    for(unsigned i=0;i<sizeof(values)/sizeof(values[0]);i++) {
        uint64_t x=values[i]; show(edges(x)); show(parallel((uint8_t)x));
        show(loop(x&127)); show(memory(x));
        for(unsigned j=0;j<sizeof(values)/sizeof(values[0]);j++) {
            uint64_t y=values[j]; show(shifts(x,y)); show(narrow(x,y)); show(arithmetic(x,y)); show(overflow(x,y));
        }
    }
    /* Include values adjacent to every signed/unsigned destination boundary,
       fractional values, infinities, signed zero and multiple NaN payloads. */
    uint64_t bits[]={0,UINT64_C(0x8000000000000000),UINT64_C(0x7ff8000000000001),UINT64_C(0xfff8000000000042)};
    double fp[]={NAN,INFINITY,-INFINITY,-1.75,1.75,-2147483649.,-2147483648.,2147483647.,2147483648.,4294967295.,4294967296.,
        -0x1p63,nextafter(-0x1p63,-INFINITY),nextafter(-0x1p63,0),nextafter(0x1p63,0),0x1p63,nextafter(0x1p64,0),0x1p64};
    for(unsigned i=0;i<sizeof(fp)/sizeof(fp[0]);i++) {
        double x=fp[i]; show(sat_s32(x));show(sat_u32(x));show(sat_s64(x));show(sat_u64(x));
        float f=(float)x; show(sat_f32_s32(f));show(sat_f32_u32(f));show(sat_f32_s64(f));show(sat_f32_u64(f));
        for(unsigned j=0;j<sizeof(fp)/sizeof(fp[0]);j++) show(nan_compare(x,fp[j]));
    }
    for(unsigned i=0;i<sizeof(bits)/sizeof(bits[0]);i++) {
        double x;memcpy(&x,&bits[i],sizeof x);
        show(sat_s32(x));show(sat_u32(x));show(sat_s64(x));show(sat_u64(x));show(nan_compare(x,1));
    }
    return 0;
}
