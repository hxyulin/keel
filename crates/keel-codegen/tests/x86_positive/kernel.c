#define _GNU_SOURCE
#include <stdint.h>
#include <string.h>
#include <stdio.h>
#include <sys/mman.h>
#include <unistd.h>
extern uint64_t rk_positive32(const void*,const void*,const void*,uint64_t);
static uint64_t state=0xfeedbeef12345678ULL;
static uint32_t next(void){state^=state<<13;state^=state>>7;state^=state<<17;return (uint32_t)state;}
int main(void){
 size_t page=(size_t)sysconf(_SC_PAGESIZE);unsigned char*p=mmap(0,3*page,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);if(p==MAP_FAILED||mprotect(p+2*page,page,PROT_NONE))return 1;
 const uint32_t edges[]={0,1,0xffffffffU,0x7fffffffU,0x80000000U};
 const unsigned lengths[]={0,1,2,3,4,5,7,8,9,15,16,17,31,32,33,257,1024};
 for(unsigned k=0;k<sizeof(lengths)/sizeof(lengths[0]);k++)for(unsigned off=0;off<4;off++)for(unsigned pattern=0;pattern<37;pattern++){
  unsigned n=lengths[k];unsigned char*src=p+2*page-4*n-off;uint64_t want=0;
  for(unsigned i=0;i<n;i++){uint32_t raw=pattern<5?edges[(pattern+i)%5]:next();int32_t v;memcpy(&v,&raw,4);memcpy(src+4*i,&raw,4);if(v>0)want+=(uint32_t)v;}
  uint64_t got=rk_positive32(src,0,0,n);if(got!=want){printf("failed n=%u off=%u pattern=%u\n",n,off,pattern);return 2;}
 }
 munmap(p,3*page);puts("kernel positive32 lengths/guard/unaligned/extrema/random passed");return 0;
}
