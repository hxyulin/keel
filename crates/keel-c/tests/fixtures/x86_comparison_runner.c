#include <immintrin.h>
#include <stdint.h>
extern int printf(const char*,...);
#define COMPARISON(name, INPUT, OUTPUT, EXPR, BYTES, BITS, COMI, ASSEMBLY) \
extern OUTPUT keel_##name(INPUT,INPUT); \
extern void keel_mem_##name(const void*,const void*,void*); \
extern void reference_mem_##name(const void*,const void*,void*); \
extern void keel_dead_##name(const void*,const void*,void*); \
static void abi_##name(const void*x,const void*y,void*out){ \
 INPUT a,b;__builtin_memcpy(&a,x,BYTES);__builtin_memcpy(&b,y,BYTES);OUTPUT r=keel_##name(a,b);__builtin_memcpy(out,&r,sizeof(r)); \
}
#include "x86_comparison_cases.h"
#undef COMPARISON
#include "x86_comparison_assembly.h"
typedef void (*Kernel)(const void*,const void*,void*);
struct Case{const char*name;Kernel actual,abi,vendor,assembly,dead;unsigned bytes,bits,comi;};
static const struct Case cases[]={
#define COMPARISON(name, INPUT, OUTPUT, EXPR, BYTES, BITS, COMI, ASSEMBLY) \
{#name,keel_mem_##name,abi_##name,reference_mem_##name,assembly_##name,keel_dead_##name,BYTES,BITS,COMI},
#include "x86_comparison_cases.h"
#undef COMPARISON
};
static uint32_t getcsr(void){uint32_t value;__asm__ volatile("stmxcsr %0":"=m"(value)::"memory");return value;}
static void setcsr(uint32_t value){__asm__ volatile("ldmxcsr %0"::"m"(value):"memory");}
static void write_word(unsigned char*p,uint64_t x,unsigned bytes){for(unsigned i=0;i<bytes;i++)p[i]=(unsigned char)(x>>(i*8));}
static const uint64_t float32[]={0,0x80000000,0x3f800000,0xbf800000,0x7f800000,0xff800000,0x7fc12345,0x7f812345,1,0x80000001};
static const uint64_t float64[]={0,0x8000000000000000ull,0x3ff0000000000000ull,0xbff0000000000000ull,0x7ff0000000000000ull,0xfff0000000000000ull,0x7ff8123456789abcull,0x7ff0123456789abcull,1,0x8000000000000001ull};
extern int keel_compare_evaluations(void);
static int exercise(void){
 if(keel_compare_evaluations()!=12){printf("comparison operands evaluated more than once\n");return 1;}
 uint32_t original=getcsr();
 for(unsigned c=0;c<sizeof(cases)/sizeof(cases[0]);c++){
  const struct Case*t=&cases[c];
  for(unsigned state=0;state<8;state++)for(unsigned sample=0;sample<100;sample++){
   uint32_t csr=0x1f80|((state&1)?0x40:0)|((state&2)?0x8000:0)|((state&4)?0x25:0);
   unsigned char x[40],y[40],expected[40],vendor[40],actual[40];
   for(unsigned i=0;i<40;i++)x[i]=y[i]=expected[i]=vendor[i]=actual[i]=0xa5;
   unsigned bytes=t->bits/8;const uint64_t*values=bytes==4?float32:float64;
   for(unsigned lane=0;lane<t->bytes/bytes;lane++){
    write_word(x+1+lane*bytes,values[(sample/10+lane)%10],bytes);
    write_word(y+1+lane*bytes,values[(sample%10+3*lane)%10],bytes);
   }
   setcsr(csr);t->assembly(x+1,y+1,expected+1);uint32_t expected_csr=getcsr();
   setcsr(csr);t->vendor(x+1,y+1,vendor+1);uint32_t vendor_csr=getcsr();
   if(vendor_csr!=expected_csr){printf("%s state %u sample %u vendor MXCSR %x expected %x\n",t->name,state,sample,vendor_csr,expected_csr);setcsr(original);return 1;}
   int compare_vendor=1;
#ifndef __clang__
   /* GCC's COMI unordered booleans differ from the pinned Clang API. */
   if(t->comi && ((sample/10==6 || sample/10==7) || (sample%10==6 || sample%10==7)))compare_vendor=0;
#endif
   if(compare_vendor)for(unsigned i=0;i<40;i++)if(vendor[i]!=expected[i]){printf("%s state %u sample %u vendor byte %u %x expected %x\n",t->name,state,sample,i,vendor[i],expected[i]);setcsr(original);return 1;}
   Kernel routes[]={t->actual,t->abi};
   for(unsigned route=0;route<2;route++){
    for(unsigned i=0;i<40;i++)actual[i]=0xa5;
    setcsr(csr);routes[route](x+1,y+1,actual+1);uint32_t actual_csr=getcsr();
    if(actual_csr!=expected_csr){printf("%s route %u state %u sample %u MXCSR %x expected %x\n",t->name,route,state,sample,actual_csr,expected_csr);setcsr(original);return 1;}
    for(unsigned i=0;i<40;i++)if(actual[i]!=expected[i]){printf("%s route %u state %u sample %u byte %u %x expected %x\n",t->name,route,state,sample,i,actual[i],expected[i]);setcsr(original);return 1;}
   }
   setcsr(csr);t->dead(x+1,y+1,actual+1);uint32_t dead_csr=getcsr();
   if(dead_csr!=expected_csr){printf("%s state %u sample %u dead MXCSR %x expected %x\n",t->name,state,sample,dead_csr,expected_csr);setcsr(original);return 1;}
  }
 }
 setcsr(original);return 0;
}
int main(void){if(!__builtin_cpu_supports("avx"))return 77;return exercise();}
