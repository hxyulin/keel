#define _GNU_SOURCE
#include <assert.h>
#include <stdint.h>
#include <string.h>
#include <math.h>
#include <stdio.h>
#include <sys/mman.h>
#include <unistd.h>

static unsigned char *edge(void) {
    long page = sysconf(_SC_PAGESIZE);
    unsigned char *p = mmap(0, (size_t)page*2, PROT_READ|PROT_WRITE, MAP_PRIVATE|MAP_ANONYMOUS, -1, 0);
    assert(p != MAP_FAILED && mprotect(p+page, (size_t)page, PROT_NONE) == 0);
    return p+page-8;
}
#define SHAPE(T,S) \
 typedef T V_##S __attribute__((vector_size(8))); \
 extern void copy_##S(const void*,void*); \
 extern void add_##S(const void*,const void*,void*); \
 typedef struct { V_##S x; } W_##S##_0; \
 typedef struct { struct { V_##S x[1]; } x; } W_##S##_1; \
 typedef struct { unsigned char empty[0]; V_##S x; } W_##S##_2;
SHAPE(uint8_t,i8x8) SHAPE(uint16_t,i16x4) SHAPE(uint32_t,i32x2)
SHAPE(uint64_t,i64x1) SHAPE(float,f32x2) SHAPE(double,f64x1)
#define WRAP(S,N) \
 extern W_##S##_##N wrap_##S##_##N(W_##S##_##N); \
 extern void call_wrap_##S##_##N(const void*,void*); \
 W_##S##_##N c_wrap_##S##_##N(W_##S##_##N x) { return x; }
#define WRAPS(S) WRAP(S,0) WRAP(S,1) WRAP(S,2)
WRAPS(i8x8) WRAPS(i16x4) WRAPS(i32x2) WRAPS(i64x1) WRAPS(f32x2)
#define CHECK_WRAP(S,N) {W_##S##_##N w,r;memcpy(&w,a,8);r=wrap_##S##_##N(w);assert(!memcmp(&w,&r,8));call_wrap_##S##_##N(&w,&r);assert(!memcmp(&w,&r,8));}
#define CHECK_MEMORY(T,S) { \
 T x[8/sizeof(T)],y[8/sizeof(T)],z[8/sizeof(T)]; \
 for(unsigned i=0;i<8/sizeof(T);i++){x[i]=(T)(i+1);y[i]=(T)(i+3);z[i]=(T)(2*i+4);} \
 memcpy(a,x,8);memcpy(b,y,8); \
 copy_##S(a,r);assert(!memcmp(a,r,8)); \
 add_##S(a,b,r);assert(!memcmp(r,z,8)); \
 add_##S(a,b,a);assert(!memcmp(a,z,8)); \
 memcpy(a,x,8);add_##S(a,b,b);assert(!memcmp(b,z,8)); \
 unsigned char u[10],v[10];memset(u,0xA5,10);memset(v,0x5A,10);memcpy(u+1,x,8); \
 copy_##S(u+1,v+1);assert(!memcmp(u+1,v+1,8)&&v[0]==0x5A&&v[9]==0x5A); \
 }
#define CHECK(T,S) { CHECK_MEMORY(T,S) CHECK_WRAP(S,0) CHECK_WRAP(S,1) CHECK_WRAP(S,2) }

#define REDUCTIONS(T,S,ST) \
 extern T vreduce_add_##S(const void*); extern T vreduce_and_##S(const void*); \
 extern T vreduce_or_##S(const void*); extern T vreduce_xor_##S(const void*); \
 extern T vreduce_smin_##S(const void*); extern T vreduce_smax_##S(const void*); \
 extern T vreduce_umin_##S(const void*); extern T vreduce_umax_##S(const void*); \
 static void reductions_##S(void) { \
 T a[8/sizeof(T)];for(unsigned k=0;k<64;k++){ \
 for(unsigned i=0;i<8/sizeof(T);i++)a[i]=(T)(UINT64_C(0xfedcba9876543210)>>(i+k)%64); \
 T sum=a[0],and=a[0],or=a[0],xor=a[0],smin=a[0],smax=a[0],umin=a[0],umax=a[0]; \
 for(unsigned i=1;i<8/sizeof(T);i++){sum=(T)((uint64_t)sum+a[i]);and&=a[i];or|=a[i];xor^=a[i];if((ST)a[i]<(ST)smin)smin=a[i];if((ST)a[i]>(ST)smax)smax=a[i];if(a[i]<umin)umin=a[i];if(a[i]>umax)umax=a[i];} \
 assert(vreduce_add_##S(a)==sum&&vreduce_and_##S(a)==and&&vreduce_or_##S(a)==or&&vreduce_xor_##S(a)==xor); \
 assert(vreduce_smin_##S(a)==smin&&vreduce_smax_##S(a)==smax&&vreduce_umin_##S(a)==umin&&vreduce_umax_##S(a)==umax);}}
REDUCTIONS(uint8_t,i8x8,int8_t) REDUCTIONS(uint16_t,i16x4,int16_t)
REDUCTIONS(uint32_t,i32x2,int32_t) REDUCTIONS(uint64_t,i64x1,int64_t)
extern void swiden_low_i8x8_i16x8(const void*,void*);
extern void uwiden_low_i8x8_i16x8(const void*,void*);
extern void narrow_low_i16x8_i8x8(const void*,void*);
extern void narrow_low_i16x4_i8x8(const void*,void*);
extern void narrow_low_i32x2_i16x4(const void*,void*);
extern void narrow_low_i64x1_i32x2(const void*,void*);
#ifdef KEEL_D8_AVX2
extern void swiden_low_i8x8_i32x8(const void*,void*);
extern void uwiden_low_i8x8_i32x8(const void*,void*);
extern void narrow_low_i16x16_i8x8(const void*,void*);
#endif
extern void fma_f32x2(const void*,const void*,const void*,void*);
extern void private_roundtrip_f64x1(const void*,void*);
extern double scalar_roundtrip_f64x1(double);
extern void fma_f64x1(const void*,const void*,const void*,void*);
int main(void) {
 unsigned char *a=edge(),*b=edge(),*r=edge();
 CHECK(uint8_t,i8x8) CHECK(uint16_t,i16x4) CHECK(uint32_t,i32x2)
 CHECK(uint64_t,i64x1) CHECK(float,f32x2) CHECK_MEMORY(double,f64x1)
 reductions_i8x8();reductions_i16x4();reductions_i32x2();reductions_i64x1();
 int8_t x[8]={-128,127,-1,0,1,-2,42,-99};int16_t q[8];int32_t y[8];uint8_t out[8];
 swiden_low_i8x8_i16x8(x,q);for(unsigned i=0;i<8;i++)assert(q[i]==x[i]);
 uwiden_low_i8x8_i16x8(x,q);for(unsigned i=0;i<8;i++)assert(q[i]==(uint8_t)x[i]);
 narrow_low_i16x8_i8x8(q,out);assert(!memcmp(out,x,8));
 memset(out,0xA5,8);narrow_low_i16x4_i8x8(q,out);for(unsigned i=0;i<8;i++)assert(out[i]==(i<4?(uint8_t)q[i]:0));
 int32_t d2[2]={-1,0x12345678};uint16_t u4[4];narrow_low_i32x2_i16x4(d2,u4);assert(u4[0]==65535&&u4[1]==0x5678&&u4[2]==0&&u4[3]==0);
 int64_t d1=INT64_C(0x123456789abcdef0);uint32_t u2[2];narrow_low_i64x1_i32x2(&d1,u2);assert(u2[0]==0x9abcdef0&&u2[1]==0);
#ifdef KEEL_D8_AVX2
 swiden_low_i8x8_i32x8(x,y);for(unsigned i=0;i<8;i++)assert(y[i]==x[i]);
 uwiden_low_i8x8_i32x8(x,y);for(unsigned i=0;i<8;i++)assert(y[i]==(uint8_t)x[i]);
 int16_t w[16];for(unsigned i=0;i<16;i++)w[i]=i*33-128;narrow_low_i16x16_i8x8(w,out);for(unsigned i=0;i<8;i++)assert(out[i]==(uint8_t)w[i]);
#else
 (void)y;
#endif
 float f[2]={0x1.000002p0f,-2.5f},g[2]={0x1.fffffep-1f,3.0f},h[2]={-1.0f,1.25f},z[2];
 fma_f32x2(f,g,h,z);for(unsigned i=0;i<2;i++)assert(z[i]==fmaf(f[i],g[i],h[i]));
 double fd=0x1.0000000000001p0,gd=0x1.fffffffffffffp-1,hd=-1.0,zd;
 fma_f64x1(&fd,&gd,&hd,&zd);assert(zd==fma(fd,gd,hd));
 double fast_in=1.25,fast_out;private_roundtrip_f64x1(&fast_in,&fast_out);assert(fast_out==2.5);assert(scalar_roundtrip_f64x1(fast_in)==2.5);
 puts("d8-boundaries-pass");return 0;
}
