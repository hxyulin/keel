#include <emmintrin.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>
#include <stdlib.h>
extern int k_evaluations, v_evaluations;
extern void k_mem0(void*,const void*), k_mem1(void*,const void*), k_mem2(void*,const void*);
extern void v_mem0(void*,const void*), v_mem1(void*,const void*), v_mem2(void*,const void*);
extern __m128 k_i2f(__m128i), k_direct(__m128i);
extern __m128i k_trunc(__m128), k_round(__m128), k_indirect(__m128,__m128(*)(__m128));
extern void k_live(__m128i), k_sequence(void*,void*,__m128i);
extern void k_dead0(__m128i), k_dead1(__m128), k_dead2(__m128);
static void (*kernels[])(void*,const void*)={k_mem0,k_mem1,k_mem2};
static void (*vendors[])(void*,const void*)={v_mem0,v_mem1,v_mem2};
static unsigned getcsr(void) { unsigned v; __asm__ volatile("stmxcsr %0":"=m"(v)::"memory");return v; }
static void setcsr(unsigned v) { __asm__ volatile("ldmxcsr %0"::"m"(v):"memory"); }
extern void k_sequence1(void*,void*,__m128), k_sequence2(void*,void*,__m128);
static uint32_t captured[8];
/* Clobber every SysV volatile SIMD register physically; SSA values must survive. */
#define CLOBBER() __asm__ volatile( \
 "pxor %%xmm0,%%xmm0\n\tpxor %%xmm1,%%xmm1\n\tpxor %%xmm2,%%xmm2\n\tpxor %%xmm3,%%xmm3\n\t" \
 "pxor %%xmm4,%%xmm4\n\tpxor %%xmm5,%%xmm5\n\tpxor %%xmm6,%%xmm6\n\tpxor %%xmm7,%%xmm7\n\t" \
 "pxor %%xmm8,%%xmm8\n\tpxor %%xmm9,%%xmm9\n\tpxor %%xmm10,%%xmm10\n\tpxor %%xmm11,%%xmm11\n\t" \
 "pxor %%xmm12,%%xmm12\n\tpxor %%xmm13,%%xmm13\n\tpxor %%xmm14,%%xmm14\n\tpxor %%xmm15,%%xmm15" \
 ::: "xmm0","xmm1","xmm2","xmm3","xmm4","xmm5","xmm6","xmm7", \
 "xmm8","xmm9","xmm10","xmm11","xmm12","xmm13","xmm14","xmm15","memory")
__attribute__((noinline)) void k_clobber(void) { CLOBBER(); }
void v_clobber(void) { k_clobber(); }
__attribute__((noinline)) __m128i k_callback(__m128i a) { CLOBBER(); return a; }
__m128i v_callback(__m128i a) { return a; }
__attribute__((noinline)) static __m128 identity(__m128 a) { CLOBBER(); return a; }
void k_capture(__m128i a,__m128 b) { memcpy(captured,&a,16);memcpy(captured+4,&b,16); }
void v_capture(__m128i a,__m128 b) { (void)a;(void)b; }
void k_change(void) { setcsr(0x1f80 | (2<<13)); }
void v_change(void) { k_change(); }
/* Integer-only IEEE-754 bit oracle. No host C numeric conversion is used. */
static int increment(unsigned rc,unsigned sign,uint64_t whole,uint64_t rem,uint64_t half) {
  if (!rem) return 0;
  if (rc==0) return rem>half || (rem==half && (whole&1));
  if (rc==1) return sign;
  if (rc==2) return !sign;
  return 0;
}
static uint32_t i2f(uint32_t x,unsigned rc,unsigned *flags) {
  unsigned sign=x>>31;
  uint32_t mag=sign?0u-x:x;
  if (!mag) return 0;
  unsigned e=31-__builtin_clz(mag);
  uint64_t sig=mag;
  if(e>23) {
    unsigned shift=e-23;
    uint64_t rem=sig&((1ull<<shift)-1);
    sig>>=shift;
    if(rem) *flags|=32;
    sig+=increment(rc,sign,sig,rem,1ull<<(shift-1));
    if(sig==(1ull<<24)) {sig>>=1;e++;}
  } else sig<<=23-e;
  return (sign<<31)|((e+127)<<23)|((uint32_t)sig&0x7fffff);
}
static uint32_t f2i(uint32_t x,unsigned rc,unsigned daz,unsigned *flags) {
  unsigned sign=x>>31, exp=(x>>23)&255;
  uint64_t sig=(x&0x7fffff)|(exp?0x800000:0);
  if(exp==255) { *flags|=1;return 0x80000000; }
  if(!exp && daz) return 0;
  if(!sig) return 0;
  int e=exp?(int)exp-127:-126;
  if(e>31) { *flags|=1;return 0x80000000; }
  uint64_t whole,rem,half;
  if(e>=23) { whole=sig<<(e-23);rem=0;half=1; }
  else {
    unsigned shift=23-e;
    if(shift>=64) {whole=0;rem=sig;half=1ull<<63;}
    else {whole=sig>>shift;rem=sig&((1ull<<shift)-1);half=1ull<<(shift-1);}
  }
  whole+=increment(rc,sign,whole,rem,half);
  if(whole>(sign?0x80000000ull:0x7fffffffull)) { *flags|=1;return 0x80000000; }
  if(rem) *flags|=32;
  return sign?0u-(uint32_t)whole:(uint32_t)whole;
}
static void reference(unsigned op,const uint32_t *in,uint32_t *out,unsigned csr,unsigned *flags) {
  for(int i=0;i<4;i++) out[i]=op==0?i2f(in[i],(csr>>13)&3,flags):f2i(in[i],op==1?3:(csr>>13)&3,(csr>>6)&1,flags);
}
static int check(unsigned op,const uint32_t *in,unsigned csr) {
  uint32_t expected[4],got[4],vendor[4];unsigned flags=csr&63;
  reference(op,in,expected,csr,&flags);
  k_evaluations=0;
  setcsr(csr);kernels[op](got,in);unsigned kc=getcsr();
  setcsr(csr);vendors[op](vendor,in);unsigned vc=getcsr();
  if(memcmp(got,expected,16)||memcmp(got,vendor,16)||kc!=(csr|flags)||vc!=kc||k_evaluations!=1) {
    fprintf(stderr,"op=%u csr=%x observed=%x vendor=%x expected flags=%x eval=%d\n",op,csr,kc,vc,flags,k_evaluations);
    for(int i=0;i<4;i++)fprintf(stderr,"%08x -> %08x expected %08x vendor %08x\n",in[i],got[i],expected[i],vendor[i]);
    return 1;
  }
  /* Dead SSA results must still produce the same sticky status. */
  __m128i vi;__m128 vf;memcpy(&vi,in,16);memcpy(&vf,in,16);
  setcsr(csr);
  if(op==0)k_dead0(vi);else if(op==1)k_dead1(vf);else k_dead2(vf);
  if(getcsr()!=kc)return 2;
  return 0;
}
static int memory(unsigned op,const uint32_t *in) {
  size_t page=(size_t)sysconf(_SC_PAGESIZE);
  unsigned char *maps[2];
  for(int i=0;i<2;i++) {
    maps[i]=mmap(0,page*3,PROT_NONE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);
    if(maps[i]==MAP_FAILED || mprotect(maps[i]+page,page,PROT_READ|PROT_WRITE))return 3;
  }
  uint32_t expected[4];unsigned flags=0;reference(op,in,expected,0x1f80,&flags);
  for(int edge=0;edge<4;edge++) {
    size_t offset=edge==0?0:edge==1?page-16:edge==2?1:page-17;
    unsigned char *src=maps[0]+page+offset,*dst=maps[1]+page+offset;
    for(int alias=0;alias<2;alias++) {
      memset(maps[0]+page,0xa5,page);memset(maps[1]+page,0xa5,page);
      memcpy(src,in,16);if(alias)dst=src;
      k_evaluations=0;setcsr(0x1f80);kernels[op](dst,src);
      if(memcmp(dst,expected,16)||k_evaluations!=1)return 4;
      for(size_t j=0;j<page;j++) {
        if((j<offset||j>=offset+16) && maps[0][page+j]!=0xa5)return 5;
        if((j<offset||j>=offset+16) && maps[1][page+j]!=0xa5)return 6;
      }
      if(!alias && memcmp(src,in,16))return 7;
    }
  }
  for(int i=0;i<2;i++)munmap(maps[i],page*3);
  return 0;
}
static int abi(void) {
  uint32_t bits[4]={0x80000000,0x7fffffff,16777217,0xfeffffff},expected[4],got[4];
  __m128i a;__m128 b;memcpy(&a,bits,16);unsigned flags=0;reference(0,bits,expected,0x1f80,&flags);
  setcsr(0x1f80);b=k_i2f(a);memcpy(got,&b,16);if(memcmp(got,expected,16))return 8;
  setcsr(0x1f80);b=k_direct(a);memcpy(got,&b,16);if(memcmp(got,expected,16))return 9;
  setcsr(0x1f80);k_live(a);if(memcmp(captured,bits,16)||memcmp(captured+4,expected,16))return 10;
  uint32_t input[4]={0x3fc00000,0xbfc00000,0x7fc00000,0xcf000000};memcpy(&b,input,16);
  for(int op=1;op<=2;op++) {
    flags=0;reference(op,input,expected,0x1f80,&flags);
    setcsr(0x1f80);a=op==1?k_trunc(b):k_round(b);memcpy(got,&a,16);if(memcmp(got,expected,16))return 11;
  }
  setcsr(0x1f80);a=k_indirect(b,identity);memcpy(got,&a,16);if(memcmp(got,expected,16))return 12;
  uint32_t first[4],second[4];memcpy(&a,bits,16);
  setcsr(0x1f80);k_sequence(first,second,a);
  flags=0;reference(0,bits,expected,0x1f80,&flags);if(memcmp(first,expected,16))return 13;
  flags=0;reference(0,bits,expected,0x5f80,&flags);if(memcmp(second,expected,16)||getcsr()!=(0x5f80|flags))return 14;
  for(unsigned op=1;op<=2;op++) {
    setcsr(0x1f80);
    if(op==1)k_sequence1(first,second,b);else k_sequence2(first,second,b);
    unsigned observed=getcsr();
    flags=0;reference(op,input,expected,0x1f80,&flags);if(memcmp(first,expected,16))return 15;
    flags=0;reference(op,input,expected,0x5f80,&flags);
    if(memcmp(second,expected,16)||observed!=(0x5f80|flags))return 16;
  }
  return 0;
}
static int run(void) {
  const uint32_t integers[]={0,1,0xffffffff,0x80000000,0x7fffffff,16777215,16777216,16777217,16777218,16777219,0xfeffffff,0xfefffffd,0x40000040,0xbfffffc0};
  const uint32_t floats[]={0,0x80000000,1,0x80000001,0x007fffff,0x807fffff,0x00800000,0x80800000,0x3f000000,0xbf000000,0x3fc00000,0xbfc00000,0x40200000,0xc0200000,0x4b800001,0xcb800001,0x4effffff,0x4f000000,0xcf000000,0xcf000001,0x7f800000,0xff800000,0x7fc00001,0x7f800001,0xffc54321,0xff854321,0x7f7fffff,0xff7fffff};
  for(unsigned op=0;op<3;op++) {
    const uint32_t *values=op==0?integers:floats;size_t n=op==0?sizeof(integers)/4:sizeof(floats)/4;
    for(unsigned rc=0;rc<4;rc++)for(unsigned daz=0;daz<2;daz++)for(unsigned ftz=0;ftz<2;ftz++)for(unsigned seed=0;seed<4;seed++) {
      unsigned csr=0x1f80|(rc<<13)|(daz<<6)|(ftz<<15)|(seed==0?0:seed==1?1:seed==2?32:63);
      for(size_t j=0;j<n;j++) {
        uint32_t in[4];for(int i=0;i<4;i++)in[i]=values[(j+i)%n];
        int result=check(op,in,csr);if(result)return result;
      }
    }
    uint32_t state=0x9e3779b9;
    for(unsigned rc=0;rc<4;rc++)for(unsigned daz=0;daz<2;daz++)for(unsigned ftz=0;ftz<2;ftz++) {
      unsigned csr=0x1f80|(rc<<13)|(daz<<6)|(ftz<<15);
      for(int j=0;j<256;j++) {
        uint32_t in[4];for(int i=0;i<4;i++) {state^=state<<13;state^=state>>17;state^=state<<5;in[i]=state;}
        int result=check(op,in,csr);if(result)return result;
      }
    }
    for(size_t j=0;j<n;j+=4) {
      uint32_t in[4];for(int i=0;i<4;i++)in[i]=values[(j+i)%n];
      int result=memory(op,in);if(result)return result;
    }
  }
  return abi();
}
int main(void) {
  unsigned saved=getcsr();int rc=run();setcsr(saved);
  if(getcsr()!=saved)return 90;
  if(rc)fprintf(stderr,"conversion gate failed: %d\n",rc);
  else puts("PASS conversions: independent bits/flags + vendor, RC/DAZ/FTZ, seeded flags, dead/live calls, single evaluation, aliases and exact16 guards");
  return rc;
}
