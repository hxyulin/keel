#include <stdint.h>
#define DECL(T,N) extern T hot##N##p(T,T,T,T); extern T hot##N##m(T,T,T,T); extern T hot##N##z(T,T,T,T)
DECL(uint32_t,32); DECL(uint64_t,64);
static const uint64_t values[]={0,1,3,7,0x7fffffffULL,0xffffffffULL,0x8000000000000000ULL,0xffffffffffffffffULL};
static const unsigned trips[]={0,1,2,3,7,16,31};
#define CHECK(T,N) do { \
 T (*functions[])(T,T,T,T)={hot##N##p,hot##N##m,hot##N##z}; \
 const T steps[]={(T)1,(T)-7,(T)0}; \
 for(unsigned f=0;f<3;f++) for(unsigned a=0;a<8;a++) for(unsigned b=0;b<8;b++) for(unsigned c=0;c<7;c++) { \
 T x=(T)values[a],stride=(T)values[b],sum=(T)17; \
 for(unsigned i=0;i<trips[c];i++) {sum+=(T)(x*stride);x+=steps[f];} \
 if(functions[f]((T)trips[c],stride,(T)values[a],(T)17)!=sum) return 1; \
 } \
} while(0)
extern uint64_t offset32(uint32_t,uint32_t,uint32_t,uint32_t);
extern uint64_t offset64(uint64_t,uint64_t,uint64_t,uint64_t);
static int check_offsets(void) {
 for(unsigned a=0;a<8;a++) for(unsigned b=0;b<8;b++)
 for(unsigned d=0;d<8;d++) for(unsigned c=0;c<7;c++) {
  uint32_t r32=(uint32_t)values[a],delta32=(uint32_t)values[d],off32=(uint32_t)values[b];
  uint64_t r64=values[a],delta64=values[d],off64=values[b],sum32=17,sum64=17;
  for(unsigned i=0;i<trips[c];i++) {
   uint32_t address=r32+off32;
   // Explicit arithmetic sign extension, without an implementation-defined cast.
   sum32+=(uint64_t)address-((address & 0x80000000U)?0x100000000ULL:0);
   sum64+=r64+off64;
   r32+=delta32; r64+=delta64;
  }
  if(offset32(trips[c],delta32,(uint32_t)values[a],off32)!=sum32) return 2;
  if(offset64(trips[c],delta64,values[a],off64)!=sum64) return 3;
 }
 return 0;
}
int main(void){CHECK(uint32_t,32);CHECK(uint64_t,64);return check_offsets();}
