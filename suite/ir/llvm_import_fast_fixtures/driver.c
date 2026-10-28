#include <stdint.h>
#include <stdio.h>
extern uint64_t entry(uint64_t);
uint64_t callback(uint64_t x) { return x ^ UINT64_C(0xa5a55a5a12345678); }
static int check(uint64_t seed) {
 uint64_t total=45;
 for(unsigned i=0;i<12;i++) total+=seed+i;
 uint64_t n=seed&7;
 uint64_t expected=(total^UINT64_C(0xa5a55a5a12345678))+total+n*(n+1)/2+(unsigned char)(seed-5)+(seed!=0);
 return entry(seed)==expected;
}
int main(void) {
 for(uint64_t i=0;i<512;i++) if(!check(i)) return 1;
 const uint64_t edge[]={UINT64_MAX,UINT64_MAX-1,UINT64_C(1)<<63,(UINT64_C(1)<<63)-1};
 for(unsigned i=0;i<4;i++) if(!check(edge[i])) return 2;
 puts("516 private fastcc patterns passed"); return 0;
}
