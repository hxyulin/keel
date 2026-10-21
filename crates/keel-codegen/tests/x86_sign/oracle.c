#define _GNU_SOURCE
#include <stdint.h>
#include <string.h>
#include <stdio.h>
#include <sys/mman.h>
#include <unistd.h>
static unsigned char *region;
static size_t page;
static uint64_t state=0xabcdef1234567890ULL;
static uint64_t next(void){state^=state<<13;state^=state>>7;state^=state<<17;return state;}
static int setup(void){
 page=(size_t)sysconf(_SC_PAGESIZE);
 region=mmap(0,2*page,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);
 return region==MAP_FAILED||mprotect(region+page,page,PROT_NONE);
}
static int check(void(*fn)(const void*,void*),unsigned complement,unsigned mode){
 const uint64_t edges[]={0,1,2,0x7fffffff,0x80000000,0xffffffff,0x100000000ULL,0x100000001ULL,0x7fffffff00000000ULL,0x7ffffffffffffffeULL,0x7fffffffffffffffULL,0x8000000000000000ULL,0x8000000000000001ULL,0xffffffff00000000ULL,UINT64_MAX};
 for(unsigned n=0;n<1200;n++)for(unsigned off=0;off<16;off++)for(unsigned alias=0;alias<2;alias++){
  uint64_t x[2],got[6];
  for(unsigned l=0;l<2;l++)x[l]=n<225?edges[(n/(l?15:1))%15]:next();
  unsigned char *src=region+page-16-off,output[49];
  memcpy(src,x,16);
  /* alias output with input; when aliasing leave room for the live-input store. */
  if(alias)src-=48;
  memcpy(src,x,16);
  unsigned char *out=alias?src:output+1;
  fn(src,out);memcpy(got,out,mode==3?48:32);
  for(unsigned l=0;l<2;l++){
   uint64_t mask=mode==1?((int64_t)x[l]>0?UINT64_MAX:0):(((x[l]>>63)^complement)?UINT64_MAX:0);
   uint64_t positive=(int64_t)x[l]>0?x[l]:0;
   uint64_t want=mode>=2?(mode==4?positive+x[l]:positive):mask;
   if(mode==3&&got[l+4]!=((int64_t)x[l]>0?UINT64_MAX:0))return 2;
   if(got[l]!=want||got[l+2]!=x[l])return 1;
  }
 }
 return 0;
}
