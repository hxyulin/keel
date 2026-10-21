#define _GNU_SOURCE
#include <stdint.h>
#include <string.h>
#include <stdio.h>
#include <sys/mman.h>
#include <unistd.h>
extern uint64_t rk_positive64(const void*,const void*,const void*,uint64_t);
extern uint64_t rk_balance64(const void*,const void*,const void*,uint64_t);
extern uint64_t rk_unsigned_high(const void*,const void*,const void*,uint64_t);
static uint64_t state=0xfeedbeef12345678ULL;
static uint64_t next(void){state^=state<<13;state^=state>>7;state^=state<<17;return state;}
int main(void){
 size_t page=(size_t)sysconf(_SC_PAGESIZE);
 unsigned char*p=mmap(0,4*page,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);
 if(p==MAP_FAILED||mprotect(p+3*page,page,PROT_NONE))return 1;
 const uint64_t edges[]={0,1,0xffffffffULL,0x100000000ULL,0x7fffffffffffffffULL,0x8000000000000000ULL,UINT64_MAX};
 const unsigned lengths[]={0,1,2,3,4,5,7,8,9,15,16,17,31,32,33,257,1024};
 for(unsigned k=0;k<sizeof(lengths)/sizeof(lengths[0]);k++)for(unsigned off=0;off<16;off++)for(unsigned pattern=0;pattern<39;pattern++){
  unsigned n=lengths[k];unsigned char*src=p+3*page-8*n-off;uint64_t want[3]={0,0,0};
  for(unsigned i=0;i<n;i++){
   uint64_t raw=pattern<7?edges[(pattern+i)%7]:next();int64_t v;memcpy(&v,&raw,8);memcpy(src+8*i,&raw,8);
   want[0]+=v>0?raw:0;want[1]+=v<0?raw:13;want[2]+=raw>INT64_MAX?raw:3;
  }
  uint64_t got[]={rk_positive64(src,src,src,n),rk_balance64(src,src,src,n),rk_unsigned_high(src,src,src,n)};
  for(unsigned j=0;j<3;j++)if(got[j]!=want[j]){printf("failed kernel=%u n=%u off=%u pattern=%u\n",j,n,off,pattern);return 2;}
 }
 munmap(p,4*page);puts("frontend sign64 lengths/guard/unaligned/extrema/random/wrap passed");return 0;
}
