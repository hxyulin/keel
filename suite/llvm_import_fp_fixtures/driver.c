#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <math.h>
static uint64_t b64(double x) { uint64_t b; memcpy(&b,&x,8); return b; }
static uint32_t b32(float x) { uint32_t b; memcpy(&b,&x,4); return b; }
static double d64(uint64_t x) { double d; memcpy(&d,&x,8); return d; }
static float d32(uint32_t x) { float d; memcpy(&d,&x,4); return d; }
static void eq64(double x,double y) { assert((isnan(x)&&isnan(y)) || b64(x)==b64(y)); }
static void eq32(float x,float y) { assert((isnan(x)&&isnan(y)) || b32(x)==b32(y)); }

extern double fadd64(double,double);
extern double fsub64(double,double);
extern double fmul64(double,double);
extern double fdiv64(double,double);
extern double neg64(double), frombits64(uint64_t);
extern uint64_t bits64(double);
extern int cmp_false64(double,double);
extern int cmp_oeq64(double,double);
extern int cmp_ogt64(double,double);
extern int cmp_oge64(double,double);
extern int cmp_olt64(double,double);
extern int cmp_ole64(double,double);
extern int cmp_one64(double,double);
extern int cmp_ord64(double,double);
extern int cmp_uno64(double,double);
extern int cmp_ueq64(double,double);
extern int cmp_ugt64(double,double);
extern int cmp_uge64(double,double);
extern int cmp_ult64(double,double);
extern int cmp_ule64(double,double);
extern int cmp_une64(double,double);
extern int cmp_true64(double,double);
extern int32_t fptosi64_i32(double);
extern int64_t fptosi64_i64(double);
extern uint32_t fptoui64_i32(double);
extern uint64_t fptoui64_i64(double);
extern double sitofp64_i8(int8_t);
extern double sitofp64_i16(int16_t);
extern double sitofp64_i32(int32_t);
extern double sitofp64_i64(int64_t);
extern double uitofp64_i8(uint8_t);
extern double uitofp64_i16(uint16_t);
extern double uitofp64_i32(uint32_t);
extern double uitofp64_i64(uint64_t);
static void check64(double a, double b) { int u=isnan(a)||isnan(b);
eq64(fadd64(a,b), a + b);
eq64(fsub64(a,b), a - b);
eq64(fmul64(a,b), a * b);
eq64(fdiv64(a,b), a / b);
assert(b64(neg64(a)) == (b64(a) ^ ((uint64_t)1 << 63)));
assert(bits64(a) == b64(a)); assert(b64(frombits64(b64(a))) == b64(a));
assert(cmp_false64(a,b) == (0));
assert(cmp_oeq64(a,b) == (a==b));
assert(cmp_ogt64(a,b) == (a>b));
assert(cmp_oge64(a,b) == (a>=b));
assert(cmp_olt64(a,b) == (a<b));
assert(cmp_ole64(a,b) == (a<=b));
assert(cmp_one64(a,b) == ((a!=b)&&!u));
assert(cmp_ord64(a,b) == (!u));
assert(cmp_uno64(a,b) == (u));
assert(cmp_ueq64(a,b) == (u||(a==b)));
assert(cmp_ugt64(a,b) == (u||(a>b)));
assert(cmp_uge64(a,b) == (u||(a>=b)));
assert(cmp_ult64(a,b) == (u||(a<b)));
assert(cmp_ule64(a,b) == (u||(a<=b)));
assert(cmp_une64(a,b) == (a!=b));
assert(cmp_true64(a,b) == (1));
}
extern float fadd32(float,float);
extern float fsub32(float,float);
extern float fmul32(float,float);
extern float fdiv32(float,float);
extern float neg32(float), frombits32(uint32_t);
extern uint32_t bits32(float);
extern int cmp_false32(float,float);
extern int cmp_oeq32(float,float);
extern int cmp_ogt32(float,float);
extern int cmp_oge32(float,float);
extern int cmp_olt32(float,float);
extern int cmp_ole32(float,float);
extern int cmp_one32(float,float);
extern int cmp_ord32(float,float);
extern int cmp_uno32(float,float);
extern int cmp_ueq32(float,float);
extern int cmp_ugt32(float,float);
extern int cmp_uge32(float,float);
extern int cmp_ult32(float,float);
extern int cmp_ule32(float,float);
extern int cmp_une32(float,float);
extern int cmp_true32(float,float);
extern int32_t fptosi32_i32(float);
extern int64_t fptosi32_i64(float);
extern uint32_t fptoui32_i32(float);
extern uint64_t fptoui32_i64(float);
extern float sitofp32_i8(int8_t);
extern float sitofp32_i16(int16_t);
extern float sitofp32_i32(int32_t);
extern float sitofp32_i64(int64_t);
extern float uitofp32_i8(uint8_t);
extern float uitofp32_i16(uint16_t);
extern float uitofp32_i32(uint32_t);
extern float uitofp32_i64(uint64_t);
static void check32(float a, float b) { int u=isnan(a)||isnan(b);
eq32(fadd32(a,b), a + b);
eq32(fsub32(a,b), a - b);
eq32(fmul32(a,b), a * b);
eq32(fdiv32(a,b), a / b);
assert(b32(neg32(a)) == (b32(a) ^ ((uint32_t)1 << 31)));
assert(bits32(a) == b32(a)); assert(b32(frombits32(b32(a))) == b32(a));
assert(cmp_false32(a,b) == (0));
assert(cmp_oeq32(a,b) == (a==b));
assert(cmp_ogt32(a,b) == (a>b));
assert(cmp_oge32(a,b) == (a>=b));
assert(cmp_olt32(a,b) == (a<b));
assert(cmp_ole32(a,b) == (a<=b));
assert(cmp_one32(a,b) == ((a!=b)&&!u));
assert(cmp_ord32(a,b) == (!u));
assert(cmp_uno32(a,b) == (u));
assert(cmp_ueq32(a,b) == (u||(a==b)));
assert(cmp_ugt32(a,b) == (u||(a>b)));
assert(cmp_uge32(a,b) == (u||(a>=b)));
assert(cmp_ult32(a,b) == (u||(a<b)));
assert(cmp_ule32(a,b) == (u||(a<=b)));
assert(cmp_une32(a,b) == (a!=b));
assert(cmp_true32(a,b) == (1));
}
extern float narrow(double), tie(void), snan32(void), tiny32(void);
extern double widen(float), memory(double,float), zero64(void), snan64(void), tiny64(void), signedzero(void);
extern double boolsigned(int), choose(int,double,double), manycall(double);
extern float boolunsigned(int), memory32(float,float,int), foldfrom(void), foldnarrow(void);
extern double many(double,double,double,double,double,double,double,double,double,double,int64_t), foldwiden(void);
extern int64_t foldsi(void);
extern uint64_t foldui(void);
extern int foldnan(void), folduno(void);
extern int8_t smallcall(int8_t);
double external_fp(double a, float b) { return a - b; }
extern double outside(double), foldneg(void);
extern uint32_t foldbits(void);
extern double indirect_fp(double (*)(double,float),double,float);
extern float indirect_fp32(float (*)(float),float);
static float fp32_identity(float x) { return x; }
int main(void) {
 eq64(indirect_fp(external_fp,2.0,0.5f),1.5);
 assert(b32(indirect_fp32(fp32_identity,-0.0f))==0x80000000);
 assert(b32(indirect_fp32(fp32_identity,d32(0x7f800001)))==0x7f800001);
 assert(outside(2.0)==1.5);
 assert(b64(foldneg())==0xfff0000000000001ULL && foldbits()==0x7f800001);

 assert(boolsigned(1)==-1.0 && boolsigned(0)==0.0);
 assert(boolunsigned(1)==1.0f && boolunsigned(0)==0.0f);
 assert(b64(choose(1,-0.0,1.0))==0x8000000000000000ULL);
 assert(b64(choose(0,1.0,-0.0))==0x8000000000000000ULL);
 assert(b32(memory32(-0.0f,1.0f,1))==0x80000000);
 assert(memory32(1.0f,2.0f,0)==2.0f);
 assert(manycall(1.0)==17.0);
 assert(many(1,2,3,4,5,6,7,8,9,10,-3)==17.0);
 assert(foldsi()==INT64_MIN && foldui()==UINT64_MAX-2047);
 assert(foldfrom()==0x1p64f && foldnarrow()==1.0f && foldwiden()==0x1p-149);
 assert(foldnan()==0 && folduno()==1);
 assert(smallcall(-128)==-128 && smallcall(127)==127);

 uint64_t vals[] = {0,0x8000000000000000ULL,1,0x8000000000000001ULL,
 0x0010000000000000ULL,0x3ff0000000000000ULL,0xbff0000000000000ULL,
 0x4004000000000000ULL,0x7fefffffffffffffULL,0x7ff0000000000000ULL,
 0xfff0000000000000ULL,0x7ff8000000000042ULL,0x7ff0000000000001ULL};
 uint32_t vals32[] = {0,0x80000000,1,0x80000001,0x00800000,0x3f800000,
 0xbf800000,0x40200000,0x7f7fffff,0x7f800000,0xff800000,0x7fc00042,0x7f800001};
 for(unsigned i=0;i<sizeof(vals)/8;i++) for(unsigned j=0;j<sizeof(vals)/8;j++) check64(d64(vals[i]),d64(vals[j]));
 for(unsigned i=0;i<sizeof(vals32)/4;i++) for(unsigned j=0;j<sizeof(vals32)/4;j++) check32(d32(vals32[i]),d32(vals32[j]));
 assert(b64(zero64())==0x8000000000000000ULL);
 assert(b64(snan64())==0x7ff0000000000001ULL);
 assert(b32(snan32())==0x7f800001); assert(b32(tiny32())==1); assert(b64(tiny64())==1);
 assert(b64(signedzero())==0x8000000000000000ULL); assert(tie()==16777216.0f);
 assert(memory(2.0,0.5f)==2.5);
 double narrow_values[] = {0.0,-0.0,0x1p-149,0x1p-150,0x1.000001p0,0x1.000003p0,INFINITY,-INFINITY,NAN};
 for(unsigned i=0;i<sizeof(narrow_values)/8;i++) eq32(narrow(narrow_values[i]),(float)narrow_values[i]);
 for(unsigned i=0;i<sizeof(vals32)/4;i++) eq64(widen(d32(vals32[i])),(double)d32(vals32[i]));

{ double v[] = { -0x1p31, nextafter(0x1p31,0.0), 0.0, -0.0, 2.75, -2.75 };
 for(unsigned i=0;i<sizeof(v)/sizeof(v[0]);i++) assert(fptosi64_i32(v[i]) == (int32_t)v[i]); }
{ double v[] = { 0.0, nextafter(0x1p32,0.0), 0.0, -0.0, 2.75, -0.75 };
 for(unsigned i=0;i<sizeof(v)/sizeof(v[0]);i++) assert(fptoui64_i32(v[i]) == (uint32_t)v[i]); }
{ double v[] = { -0x1p63, nextafter(0x1p63,0.0), 0.0, -0.0, 2.75, -2.75 };
 for(unsigned i=0;i<sizeof(v)/sizeof(v[0]);i++) assert(fptosi64_i64(v[i]) == (int64_t)v[i]); }
{ double v[] = { 0.0, nextafter(0x1p64,0.0), 0.0, -0.0, 2.75, -0.75 };
 for(unsigned i=0;i<sizeof(v)/sizeof(v[0]);i++) assert(fptoui64_i64(v[i]) == (uint64_t)v[i]); }
{ int8_t v[] = {0,1,(int8_t)-1,(int8_t)((uint64_t)1<<7),(int8_t)16777217,(int8_t)9007199254740993ULL};
 for(unsigned i=0;i<sizeof(v)/sizeof(v[0]);i++) eq64(sitofp64_i8(v[i]),(double)v[i]); }
{ int16_t v[] = {0,1,(int16_t)-1,(int16_t)((uint64_t)1<<15),(int16_t)16777217,(int16_t)9007199254740993ULL};
 for(unsigned i=0;i<sizeof(v)/sizeof(v[0]);i++) eq64(sitofp64_i16(v[i]),(double)v[i]); }
{ int32_t v[] = {0,1,(int32_t)-1,(int32_t)((uint64_t)1<<31),(int32_t)16777217,(int32_t)9007199254740993ULL};
 for(unsigned i=0;i<sizeof(v)/sizeof(v[0]);i++) eq64(sitofp64_i32(v[i]),(double)v[i]); }
{ int64_t v[] = {0,1,(int64_t)-1,(int64_t)((uint64_t)1<<63),(int64_t)16777217,(int64_t)9007199254740993ULL};
 for(unsigned i=0;i<sizeof(v)/sizeof(v[0]);i++) eq64(sitofp64_i64(v[i]),(double)v[i]); }
{ uint8_t v[] = {0,1,(uint8_t)-1,(uint8_t)((uint64_t)1<<7),(uint8_t)16777217,(uint8_t)9007199254740993ULL};
 for(unsigned i=0;i<sizeof(v)/sizeof(v[0]);i++) eq64(uitofp64_i8(v[i]),(double)v[i]); }
{ uint16_t v[] = {0,1,(uint16_t)-1,(uint16_t)((uint64_t)1<<15),(uint16_t)16777217,(uint16_t)9007199254740993ULL};
 for(unsigned i=0;i<sizeof(v)/sizeof(v[0]);i++) eq64(uitofp64_i16(v[i]),(double)v[i]); }
{ uint32_t v[] = {0,1,(uint32_t)-1,(uint32_t)((uint64_t)1<<31),(uint32_t)16777217,(uint32_t)9007199254740993ULL};
 for(unsigned i=0;i<sizeof(v)/sizeof(v[0]);i++) eq64(uitofp64_i32(v[i]),(double)v[i]); }
{ uint64_t v[] = {0,1,(uint64_t)-1,(uint64_t)((uint64_t)1<<63),(uint64_t)16777217,(uint64_t)9007199254740993ULL};
 for(unsigned i=0;i<sizeof(v)/sizeof(v[0]);i++) eq64(uitofp64_i64(v[i]),(double)v[i]); }
{ float v[] = { -0x1p31, nextafterf(0x1p31,0.0), 0.0, -0.0, 2.75, -2.75 };
 for(unsigned i=0;i<sizeof(v)/sizeof(v[0]);i++) assert(fptosi32_i32(v[i]) == (int32_t)v[i]); }
{ float v[] = { 0.0, nextafterf(0x1p32,0.0), 0.0, -0.0, 2.75, -0.75 };
 for(unsigned i=0;i<sizeof(v)/sizeof(v[0]);i++) assert(fptoui32_i32(v[i]) == (uint32_t)v[i]); }
{ float v[] = { -0x1p63, nextafterf(0x1p63,0.0), 0.0, -0.0, 2.75, -2.75 };
 for(unsigned i=0;i<sizeof(v)/sizeof(v[0]);i++) assert(fptosi32_i64(v[i]) == (int64_t)v[i]); }
{ float v[] = { 0.0, nextafterf(0x1p64,0.0), 0.0, -0.0, 2.75, -0.75 };
 for(unsigned i=0;i<sizeof(v)/sizeof(v[0]);i++) assert(fptoui32_i64(v[i]) == (uint64_t)v[i]); }
{ int8_t v[] = {0,1,(int8_t)-1,(int8_t)((uint64_t)1<<7),(int8_t)16777217,(int8_t)9007199254740993ULL};
 for(unsigned i=0;i<sizeof(v)/sizeof(v[0]);i++) eq32(sitofp32_i8(v[i]),(float)v[i]); }
{ int16_t v[] = {0,1,(int16_t)-1,(int16_t)((uint64_t)1<<15),(int16_t)16777217,(int16_t)9007199254740993ULL};
 for(unsigned i=0;i<sizeof(v)/sizeof(v[0]);i++) eq32(sitofp32_i16(v[i]),(float)v[i]); }
{ int32_t v[] = {0,1,(int32_t)-1,(int32_t)((uint64_t)1<<31),(int32_t)16777217,(int32_t)9007199254740993ULL};
 for(unsigned i=0;i<sizeof(v)/sizeof(v[0]);i++) eq32(sitofp32_i32(v[i]),(float)v[i]); }
{ int64_t v[] = {0,1,(int64_t)-1,(int64_t)((uint64_t)1<<63),(int64_t)16777217,(int64_t)9007199254740993ULL};
 for(unsigned i=0;i<sizeof(v)/sizeof(v[0]);i++) eq32(sitofp32_i64(v[i]),(float)v[i]); }
{ uint8_t v[] = {0,1,(uint8_t)-1,(uint8_t)((uint64_t)1<<7),(uint8_t)16777217,(uint8_t)9007199254740993ULL};
 for(unsigned i=0;i<sizeof(v)/sizeof(v[0]);i++) eq32(uitofp32_i8(v[i]),(float)v[i]); }
{ uint16_t v[] = {0,1,(uint16_t)-1,(uint16_t)((uint64_t)1<<15),(uint16_t)16777217,(uint16_t)9007199254740993ULL};
 for(unsigned i=0;i<sizeof(v)/sizeof(v[0]);i++) eq32(uitofp32_i16(v[i]),(float)v[i]); }
{ uint32_t v[] = {0,1,(uint32_t)-1,(uint32_t)((uint64_t)1<<31),(uint32_t)16777217,(uint32_t)9007199254740993ULL};
 for(unsigned i=0;i<sizeof(v)/sizeof(v[0]);i++) eq32(uitofp32_i32(v[i]),(float)v[i]); }
{ uint64_t v[] = {0,1,(uint64_t)-1,(uint64_t)((uint64_t)1<<63),(uint64_t)16777217,(uint64_t)9007199254740993ULL};
 for(unsigned i=0;i<sizeof(v)/sizeof(v[0]);i++) eq32(uitofp32_i64(v[i]),(float)v[i]); }
puts("scalar-fp independent assertions passed"); return 0; }
