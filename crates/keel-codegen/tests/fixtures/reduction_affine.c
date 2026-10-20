#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>
extern uint64_t affine(void*,uint64_t,uint64_t,uint64_t,uint64_t);
int main(void) {
 long page=sysconf(_SC_PAGESIZE);
 unsigned char *region=mmap(0,2*page,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANON,-1,0);
 if(region==MAP_FAILED || mprotect(region+page,page,PROT_NONE)) return 2;
 uint64_t offsets[]={0,1,3,17,UINT64_MAX,UINT64_MAX-63,UINT64_C(1)<<61,UINT64_C(1)<<63};
 unsigned long checks=0;
 for(unsigned n=0;n<=67;n++) {
  uint64_t *data=(void*)(region+page-n*8);
  for(unsigned i=0;i<n;i++) data[i]=UINT64_C(0x8000fedc12345678) ^ ((uint64_t)i*UINT64_C(0xfedcba987654321));
  for(unsigned o=0;o<sizeof(offsets)/sizeof(*offsets);o++) {
   uint64_t offset=offsets[o];
   uintptr_t base=(uintptr_t)data-offset*8;
   for(unsigned start=0;start<=n+1;start++) for(unsigned k=0;k<3;k++) {
    uint64_t seed=k==0?0:k==1?UINT64_MAX:UINT64_C(0x8000000000000000),want=seed;
    for(uint64_t i=start;i<n;i++) { uint64_t x; uintptr_t address=base+(offset+i)*8; memcpy(&x,(void*)address,8); want+=x; }
    uint64_t got=affine((void*)base,n,offset,start,seed);
    if(got!=want) { fprintf(stderr,"n=%u start=%u offset=%llu\n",n,start,(unsigned long long)offset); return 1; }
    checks++;
   }
  }
 }
 printf("affine: %lu oracle comparisons\n",checks);
 return 0;
}
