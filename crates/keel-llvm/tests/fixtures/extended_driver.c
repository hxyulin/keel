#include <stdint.h>
#include <stdio.h>
#include <math.h>
#include <string.h>
uint64_t external_value=73;
uint64_t helper(uint64_t x) {return x^UINT64_C(0x935a54d2c711e8f0);}
extern uint64_t dynamic(uint64_t),memory_more(uint64_t),mixed(uint64_t,void*,double);
static void show(uint64_t x) {printf("%016llx\n",(unsigned long long)x);}
static uint64_t dbits(double x){uint64_t b;memcpy(&b,&x,8);return b;}
static uint32_t fbits(float x){uint32_t b;memcpy(&b,&x,4);return b;}
extern uint64_t constant_32_0(void);
extern uint64_t constant_32_1(void);
extern uint64_t constant_32_2(void);
extern uint64_t constant_32_3(void);
extern uint64_t constant_32_4(void);
extern uint64_t constant_32_5(void);
extern uint64_t constant_32_6(void);
extern uint64_t constant_32_7(void);
extern uint64_t constant_32_8(void);
extern uint64_t fmin_32(float,float);
extern uint64_t fmax_32(float,float);
extern uint64_t fcopysign_32(float,float);
extern uint64_t fneg_32(float);
extern uint64_t fabs_32(float);
extern uint64_t fsqrt_32(float);
extern uint64_t fceil_32(float);
extern uint64_t ffloor_32(float);
extern uint64_t ftrunc_32(float);
extern uint64_t fnearest_32(float);
extern uint64_t fused_32(float,float,float);
extern uint64_t constant_64_0(void);
extern uint64_t constant_64_1(void);
extern uint64_t constant_64_2(void);
extern uint64_t constant_64_3(void);
extern uint64_t constant_64_4(void);
extern uint64_t constant_64_5(void);
extern uint64_t constant_64_6(void);
extern uint64_t constant_64_7(void);
extern uint64_t constant_64_8(void);
extern uint64_t fmin_64(double,double);
extern uint64_t fmax_64(double,double);
extern uint64_t fcopysign_64(double,double);
extern uint64_t fneg_64(double);
extern uint64_t fabs_64(double);
extern uint64_t fsqrt_64(double);
extern uint64_t fceil_64(double);
extern uint64_t ffloor_64(double);
extern uint64_t ftrunc_64(double);
extern uint64_t fnearest_64(double);
extern uint64_t fused_64(double,double,double);
extern uint64_t overflow_uadd_8(uint64_t,uint64_t);
extern uint64_t overflow_sadd_8(uint64_t,uint64_t);
extern uint64_t overflow_usub_8(uint64_t,uint64_t);
extern uint64_t overflow_ssub_8(uint64_t,uint64_t);
extern uint64_t overflow_umul_8(uint64_t,uint64_t);
extern uint64_t overflow_smul_8(uint64_t,uint64_t);
extern uint64_t clz_8(uint64_t);
extern uint64_t ctz_8(uint64_t);
extern uint64_t popcnt_8(uint64_t);
extern uint64_t overflow_uadd_16(uint64_t,uint64_t);
extern uint64_t overflow_sadd_16(uint64_t,uint64_t);
extern uint64_t overflow_usub_16(uint64_t,uint64_t);
extern uint64_t overflow_ssub_16(uint64_t,uint64_t);
extern uint64_t overflow_umul_16(uint64_t,uint64_t);
extern uint64_t overflow_smul_16(uint64_t,uint64_t);
extern uint64_t clz_16(uint64_t);
extern uint64_t ctz_16(uint64_t);
extern uint64_t popcnt_16(uint64_t);
extern uint64_t bswap_16(uint64_t);
extern uint64_t overflow_uadd_32(uint64_t,uint64_t);
extern uint64_t overflow_sadd_32(uint64_t,uint64_t);
extern uint64_t overflow_usub_32(uint64_t,uint64_t);
extern uint64_t overflow_ssub_32(uint64_t,uint64_t);
extern uint64_t overflow_umul_32(uint64_t,uint64_t);
extern uint64_t overflow_smul_32(uint64_t,uint64_t);
extern uint64_t clz_32(uint64_t);
extern uint64_t ctz_32(uint64_t);
extern uint64_t popcnt_32(uint64_t);
extern uint64_t bswap_32(uint64_t);
extern uint64_t overflow_uadd_64(uint64_t,uint64_t);
extern uint64_t overflow_sadd_64(uint64_t,uint64_t);
extern uint64_t overflow_usub_64(uint64_t,uint64_t);
extern uint64_t overflow_ssub_64(uint64_t,uint64_t);
extern uint64_t overflow_umul_64(uint64_t,uint64_t);
extern uint64_t overflow_smul_64(uint64_t,uint64_t);
extern uint64_t clz_64(uint64_t);
extern uint64_t ctz_64(uint64_t);
extern uint64_t popcnt_64(uint64_t);
extern uint64_t bswap_64(uint64_t);
extern uint64_t overflow_uadd_128(uint64_t,uint64_t);
extern uint64_t overflow_sadd_128(uint64_t,uint64_t);
extern uint64_t overflow_usub_128(uint64_t,uint64_t);
extern uint64_t overflow_ssub_128(uint64_t,uint64_t);
extern uint64_t overflow_umul_128(uint64_t,uint64_t);
extern uint64_t overflow_smul_128(uint64_t,uint64_t);
extern uint64_t clz_128(uint64_t);
extern uint64_t ctz_128(uint64_t);
extern uint64_t popcnt_128(uint64_t);
extern uint64_t bswap_128(uint64_t);
int main(void) {
    uint64_t values[]={0,1,7,8,127,128,255,256,32767,32768,65535,UINT64_MAX,UINT64_C(0x8000000000000000)};
    for(unsigned i=0;i<sizeof(values)/sizeof(values[0]);i++) {
        uint64_t x=values[i];show(dynamic(x&31));show(memory_more(x));
        show(mixed(x&31,&external_value,17.75));
        show(clz_8(x));
        show(ctz_8(x));
        show(popcnt_8(x));
        show(clz_16(x));
        show(ctz_16(x));
        show(popcnt_16(x));
        show(bswap_16(x));
        show(clz_32(x));
        show(ctz_32(x));
        show(popcnt_32(x));
        show(bswap_32(x));
        show(clz_64(x));
        show(ctz_64(x));
        show(popcnt_64(x));
        show(bswap_64(x));
        show(clz_128(x));
        show(ctz_128(x));
        show(popcnt_128(x));
        show(bswap_128(x));
        for(unsigned j=0;j<sizeof(values)/sizeof(values[0]);j++) {
            uint64_t y=values[j];
            show(overflow_uadd_8(x,y));
            show(overflow_sadd_8(x,y));
            show(overflow_usub_8(x,y));
            show(overflow_ssub_8(x,y));
            show(overflow_umul_8(x,y));
            show(overflow_smul_8(x,y));
            show(overflow_uadd_16(x,y));
            show(overflow_sadd_16(x,y));
            show(overflow_usub_16(x,y));
            show(overflow_ssub_16(x,y));
            show(overflow_umul_16(x,y));
            show(overflow_smul_16(x,y));
            show(overflow_uadd_32(x,y));
            show(overflow_sadd_32(x,y));
            show(overflow_usub_32(x,y));
            show(overflow_ssub_32(x,y));
            show(overflow_umul_32(x,y));
            show(overflow_smul_32(x,y));
            show(overflow_uadd_64(x,y));
            show(overflow_sadd_64(x,y));
            show(overflow_usub_64(x,y));
            show(overflow_ssub_64(x,y));
            show(overflow_umul_64(x,y));
            show(overflow_smul_64(x,y));
            show(overflow_uadd_128(x,y));
            show(overflow_sadd_128(x,y));
            show(overflow_usub_128(x,y));
            show(overflow_ssub_128(x,y));
        }
    }
    show(constant_32_0());
    show(constant_32_1());
    show(constant_32_2());
    show(constant_32_3());
    show(constant_32_4());
    show(constant_32_5());
    show(constant_32_6());
    show(constant_32_7());
    show(constant_32_8());
    show(constant_64_0());
    show(constant_64_1());
    show(constant_64_2());
    show(constant_64_3());
    show(constant_64_4());
    show(constant_64_5());
    show(constant_64_6());
    show(constant_64_7());
    show(constant_64_8());
    double v[]={0.,-0.,1.,-1.,2.5,-2.5,3.5,-3.5,INFINITY,-INFINITY,NAN};
    for(unsigned i=0;i<sizeof(v)/sizeof(v[0]);i++) {
        double x=v[i];
        show(fneg_32((float)x));
        show(fabs_32((float)x));
        if(!isnan(x)) show(fceil_32((float)x));
        if(!isnan(x)) show(ffloor_32((float)x));
        if(!isnan(x)) show(ftrunc_32((float)x));
        if(!isnan(x)) show(fnearest_32((float)x));
        if(x>=0) show(fsqrt_32((float)x));
        show(fneg_64(x));
        show(fabs_64(x));
        if(!isnan(x)) show(fceil_64(x));
        if(!isnan(x)) show(ffloor_64(x));
        if(!isnan(x)) show(ftrunc_64(x));
        if(!isnan(x)) show(fnearest_64(x));
        if(x>=0) show(fsqrt_64(x));
        for(unsigned j=0;j<sizeof(v)/sizeof(v[0]);j++) {
            double y=v[j];
            show(fcopysign_32((float)x,(float)y));
            { uint64_t raw=fmin_32((float)x,(float)y); uint32_t b=(uint32_t)raw; float r;memcpy(&r,&b,4);show(isnan(r)?UINT64_C(0x7ff8000000000000):raw); }
            { uint64_t raw=fmax_32((float)x,(float)y); uint32_t b=(uint32_t)raw; float r;memcpy(&r,&b,4);show(isnan(r)?UINT64_C(0x7ff8000000000000):raw); }
            show(fcopysign_64(x,y));
            { uint64_t raw=fmin_64(x,y); uint64_t b=(uint64_t)raw; double r;memcpy(&r,&b,8);show(isnan(r)?UINT64_C(0x7ff8000000000000):raw); }
            { uint64_t raw=fmax_64(x,y); uint64_t b=(uint64_t)raw; double r;memcpy(&r,&b,8);show(isnan(r)?UINT64_C(0x7ff8000000000000):raw); }
        }
    }
    /* Fused cancellation produces a nonzero result; unfused product gives 0. */
    show(fused_64(0x1.0000000000001p0,0x1.fffffffffffffp-1,-1.));
    show(fused_32(0x1.000002p0f,0x1.fffffep-1f,-1.f));
    if(fused_64(0x1.0000000000001p0,0x1.fffffffffffffp-1,-1.)!=dbits(fma(0x1.0000000000001p0,0x1.fffffffffffffp-1,-1.))) return 1;
    if(fused_32(0x1.000002p0f,0x1.fffffep-1f,-1.f)!=fbits(fmaf(0x1.000002p0f,0x1.fffffep-1f,-1.f))) return 2;
    return 0;
}
