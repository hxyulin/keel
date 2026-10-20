#include <stdint.h>
#include <assert.h>
#include <stdio.h>
static unsigned alias_mode;
#define INIT(B,T) \
 static T alternate##B; \
 void *init##B(void *p, uint64_t seed) { *(T *)p=(T)seed; alternate##B=(T)(seed^0x517cc1b727220a95ULL); return alias_mode?p:&alternate##B; } \
 extern uint64_t cache##B(uint64_t,uint64_t);
INIT(8,uint8_t)
INIT(16,uint16_t)
INIT(32,uint32_t)
INIT(64,uint64_t)
int main(void) {
 const uint64_t seeds[]={0,1,127,255,65535,0xffffffffULL,1ULL<<63,~0ULL};
 const unsigned sizes[]={0,1,2,7,16,63,128};
 unsigned cases=0;
 for(alias_mode=0;alias_mode<2;alias_mode++) for(unsigned s=0;s<8;s++) for(unsigned j=0;j<7;j++) {
  uint64_t seed=seeds[s],n=sizes[j];
  #define CHECK(B,T) { uint64_t sum=0;for(uint64_t i=0;i<n;i++)sum+=alias_mode?(T)(seed+i+1):(T)(seed^0x517cc1b727220a95ULL); uint64_t expected=sum+(T)(seed+n); assert(cache##B(n,seed)==expected);cases++; }
  CHECK(8,uint8_t) CHECK(16,uint16_t) CHECK(32,uint32_t) CHECK(64,uint64_t)
 }
 printf("%u alias/disjoint, zero-trip and wrapping cases passed\n",cases);
}
