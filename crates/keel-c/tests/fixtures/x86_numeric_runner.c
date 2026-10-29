#include <immintrin.h>
#include <stdint.h>
extern int printf(const char*,...);
#define CONVERSION(name, INPUT, OUTPUT, EXPR, INPUT_BYTES, OUTPUT_BYTES, KIND, ASSEMBLY) \
extern OUTPUT keel##name(INPUT,OUTPUT); \
extern OUTPUT reference##name(INPUT,OUTPUT); \
extern void keelmem##name(const void*,const void*,void*); \
extern void referencemem##name(const void*,const void*,void*); \
extern void keeldead##name(const void*,const void*,void*); \
static void abi##name(const void*input,const void*merge,void*out) { \
  INPUT b; OUTPUT a; __builtin_memcpy(&b,input,sizeof(b));__builtin_memcpy(&a,merge,sizeof(a)); \
  OUTPUT r=keel##name(b,a);__builtin_memcpy(out,&r,sizeof(r)); \
}
#include "x86_numeric_cases.h"
#undef CONVERSION
#include "x86_numeric_assembly.h"
typedef void (*Kernel)(const void*,const void*,void*);
struct Case {const char*name;Kernel actual,abi,vendor,assembly,dead;unsigned input_bytes,output_bytes,kind;};
static const struct Case cases[]={
#define CONVERSION(name, INPUT, OUTPUT, EXPR, INPUT_BYTES, OUTPUT_BYTES, KIND, ASSEMBLY) \
{#name,keelmem##name,abi##name,referencemem##name,assembly##name,keeldead##name,INPUT_BYTES,OUTPUT_BYTES,KIND},
#include "x86_numeric_cases.h"
#undef CONVERSION
};
static uint32_t getcsr(void){uint32_t value;__asm__ volatile("stmxcsr %0":"=m"(value)::"memory");return value;}
static void setcsr(uint32_t value){__asm__ volatile("ldmxcsr %0"::"m"(value):"memory");}
static void write_word(unsigned char*p,uint64_t x,unsigned bytes){for(unsigned i=0;i<bytes;i++)p[i]=(unsigned char)(x>>(i*8));}
static const uint64_t integer32[]={0,1,0x7fffffff,0x80000000,0xffffffff,16777217,0xff000001};
static const uint64_t integer64[]={0,1,0x7fffffffffffffffull,0x8000000000000000ull,0xffffffffffffffffull,9007199254740993ull,16777217};
static const uint64_t float32[]={0,0x80000000,0x3fc00000,0xbfc00000,0x40200000,0xc0200000,0x4f000000,0xcf000000,0x5f000000,0xdf000000,0x7f800000,0xff800000,0x7fc12345,0x7f812345,1,0x80000001,0x7f7fffff,0x3f000001};
static const uint64_t float64[]={0,0x8000000000000000ull,0x3ff8000000000000ull,0xbff8000000000000ull,0x4004000000000000ull,0xc004000000000000ull,0x41e0000000000000ull,0xc1e0000000000000ull,0x43e0000000000000ull,0xc3e0000000000000ull,0x7ff0000000000000ull,0xfff0000000000000ull,0x7ff8123456789abcull,0x7ff0123456789abcull,1,0x8000000000000001ull,0x7fefffffffffffffull,0x3ff0000010000000ull};
static int exercise(void){
 uint32_t original=getcsr();
 for(unsigned c=0;c<sizeof(cases)/sizeof(cases[0]);c++){
  const struct Case*t=&cases[c];
  for(unsigned state=0;state<32;state++)for(unsigned sample=0;sample<18;sample++){
   uint32_t csr=0x1f80|((state&3)<<13)|((state&4)?0x40:0)|((state&8)?0x8000:0)|((state&16)?0x25:0);
   unsigned char input[40],merge[40],expected[40],vendor[40],actual[40];
   for(unsigned i=0;i<40;i++)input[i]=merge[i]=expected[i]=vendor[i]=actual[i]=0xa5;
   unsigned lane_bytes=t->kind==1||t->kind==3?8:4;
   unsigned count=t->kind<2?7:18;
   const uint64_t*values=t->kind==0?integer32:t->kind==1?integer64:t->kind==2?float32:float64;
   for(unsigned lane=0;lane<t->input_bytes/lane_bytes;lane++)write_word(input+1+lane*lane_bytes,values[(sample+lane)%count],lane_bytes);
   for(unsigned i=0;i<32;i+=4)write_word(merge+1+i,(i&4)?0x80000000:0x7f812345,4);
   setcsr(csr);t->assembly(input+1,merge+1,expected+1);uint32_t expected_csr=getcsr();
   setcsr(csr);t->vendor(input+1,merge+1,vendor+1);uint32_t vendor_csr=getcsr();
   if(vendor_csr!=expected_csr){printf("%s state %u sample %u vendor MXCSR %x expected %x\n",t->name,state,sample,vendor_csr,expected_csr);setcsr(original);return 1;}
   for(unsigned i=0;i<40;i++)if(vendor[i]!=expected[i]){printf("%s state %u sample %u vendor byte %u %x expected %x\n",t->name,state,sample,i,vendor[i],expected[i]);setcsr(original);return 1;}
   Kernel routes[]={t->actual,t->abi};
   for(unsigned route=0;route<2;route++){
    for(unsigned i=0;i<40;i++)actual[i]=0xa5;
    setcsr(csr);routes[route](input+1,merge+1,actual+1);uint32_t actual_csr=getcsr();
    if(actual_csr!=expected_csr){printf("%s route %u state %u sample %u MXCSR %x expected %x\n",t->name,route,state,sample,actual_csr,expected_csr);setcsr(original);return 1;}
    for(unsigned i=0;i<40;i++)if(actual[i]!=expected[i]){printf("%s route %u state %u sample %u byte %u %x expected %x\n",t->name,route,state,sample,i,actual[i],expected[i]);setcsr(original);return 1;}
   }
   for(unsigned alias=0;alias<2;alias++){
    unsigned char storage[40],before[40];
    for(unsigned i=0;i<40;i++)storage[i]=before[i]=alias?merge[i]:input[i];
    setcsr(csr);t->actual(alias?input+1:storage+1,alias?storage+1:merge+1,storage+1);uint32_t alias_csr=getcsr();
    if(alias_csr!=expected_csr){printf("%s alias %u MXCSR %x expected %x\n",t->name,alias,alias_csr,expected_csr);setcsr(original);return 1;}
    for(unsigned i=0;i<40;i++){
     unsigned want=i>0&&i<=t->output_bytes?expected[i]:before[i];
     if(storage[i]!=want){printf("%s alias %u byte %u %x expected %x\n",t->name,alias,i,storage[i],want);setcsr(original);return 1;}
    }
   }
   setcsr(csr);t->dead(input+1,merge+1,actual+1);uint32_t dead_csr=getcsr();
   if(dead_csr!=expected_csr){printf("%s state %u sample %u dead MXCSR %x expected %x\n",t->name,state,sample,dead_csr,expected_csr);setcsr(original);return 1;}
  }
 }
 setcsr(original);return 0;
}
int main(void){if(!__builtin_cpu_supports("avx"))return 77;return exercise();}
