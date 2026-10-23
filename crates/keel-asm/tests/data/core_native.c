#define _GNU_SOURCE
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>
#ifndef MAP_32BIT
#define MAP_32BIT 0x40 /* Linux x86_64 flag; permits host syntax checking only. */
#endif
extern uint64_t core_rcl(uint64_t,uint64_t,uint8_t*),core_rcr(uint64_t,uint64_t,uint8_t*);
extern uint64_t core_loop(uint64_t),core_loope(uint64_t),core_loopne(uint64_t);
extern uint64_t core_jecxz(uint64_t),core_jrcxz(uint64_t);
extern void core_copy(void*,const void*,uint64_t),core_stosq(void*,uint64_t,uint64_t),core_flush(void*);
extern uint64_t core_copy_df(void*,const void*,uint64_t),core_lodsb(const void*);
extern uint64_t core_cmps(const void*,const void*,uint64_t),core_scas(const void*,uint64_t,uint8_t);
extern uint64_t core_rep_lods(const void*,uint64_t);
extern uint64_t core_xlat(const void*,uint64_t),core_cx8(uint64_t*,uint64_t,uint64_t);
extern uint64_t core_cwd(uint64_t),core_enter(void);
extern uint64_t core_addr32_load(uint64_t);
extern void core_addr32_store(uint64_t,uint32_t);
#define CHECK(x) do { if (!(x)) { fprintf(stderr,"core smoke line %d\n",__LINE__); return __LINE__; } } while(0)
static unsigned char *guarded(size_t page) {
  unsigned char *p=mmap(0,2*page,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);
  if(p==MAP_FAILED || mprotect(p+page,page,PROT_NONE)) return 0;
  return p;
}
int main(void) {
#if defined(__x86_64__)
  unsigned ca=0,cb,cc,cd;
  __asm__ volatile("cpuid":"+a"(ca),"=b"(cb),"=c"(cc),"=d"(cd):"c"(0));
  CHECK(ca>=1); ca=1;
  __asm__ volatile("cpuid":"+a"(ca),"=b"(cb),"=c"(cc),"=d"(cd):"c"(0));
  if (!(cd & (1u<<19))) { fputs("required CPUID.CLFSH missing\n",stderr); return 77; }
#endif
  uint8_t carry=9;
  const uint64_t values[]={0,1,2,3,0x7fffffffffffffffULL,0x8000000000000000ULL,0xffffffffffffffffULL};
  for(unsigned i=0;i<sizeof(values)/sizeof(values[0]);i++) for(unsigned c=0;c<2;c++) {
    uint64_t x=values[i];
    CHECK(core_rcl(x,c,&carry)==((x<<1)|c) && carry==(x>>63));
    CHECK(core_rcr(x,c,&carry)==((x>>1)|((uint64_t)c<<63)) && carry==(x&1));
  }
  CHECK(core_rcl(0x8000000000000000ULL,1,&carry)==1 && carry==1);
  CHECK(core_rcl(3,0,&carry)==6 && carry==0);
  CHECK(core_rcr(1,1,&carry)==0x8000000000000000ULL && carry==1);
  CHECK(core_loop(3)==3 && core_loope(5)==2 && core_loopne(5)==2);
  CHECK(core_jecxz(0x100000000ULL)==2 && core_jrcxz(0x100000000ULL)==1);
  CHECK(core_jecxz(0)==2 && core_jrcxz(0)==2);
  size_t page=(size_t)sysconf(_SC_PAGESIZE);
  unsigned char *low=mmap(0,2*page,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANONYMOUS|MAP_32BIT,-1,0);
  CHECK(low!=MAP_FAILED && (uintptr_t)(low+2*page)<=UINT32_MAX);
  CHECK(mprotect(low+page,page,PROT_NONE)==0);
  // Address-size 32 truncates the incoming high half before memory access.
  uint64_t wrapped=(uint64_t)(uintptr_t)(low+page-4) | (1ULL<<32);
  core_addr32_store(wrapped,0x89abcdefU);
  CHECK(core_addr32_load(wrapped)==0x89abcdefU);
  CHECK(munmap(low,2*page)==0);
  unsigned char *a=guarded(page),*b=guarded(page);
  CHECK(a && b);
  unsigned char *src=a+page-256,*dst=b+page-256;
  for(int i=0;i<256;i++) src[i]=(unsigned char)(i^0x5a);
  // REP with zero count must not touch inaccessible guard pages.
  core_copy(b+page,a+page,0);
  core_stosq(b+page,0x1234,0);
  core_copy(dst+240,src+240,16);
  CHECK(memcmp(dst+240,src+240,16)==0);
  memset(dst+240,0,16);
  CHECK(core_copy_df(dst+240,src+240,16)==1);
  CHECK(memcmp(dst+240,src+240,16)==0);
  CHECK(core_lodsb(src+255)==src[255]);
  CHECK(core_rep_lods(src+240,16)==src[255]);
  CHECK(core_cmps(src+240,dst+240,16)==1);
  dst[255]^=1;
  CHECK(core_cmps(src+240,dst+240,16)==0);
  CHECK(core_scas(src+240,16,src[255])==16);
  CHECK(core_xlat(src,255)==src[255]);
  core_stosq(dst+240,0x123456789abcdef0ULL,2);
  CHECK(((uint64_t*)(dst+240))[0]==0x123456789abcdef0ULL && ((uint64_t*)(dst+240))[1]==0x123456789abcdef0ULL);
  uint64_t word=0x1122334455667788ULL;
  CHECK(core_cx8(&word,0x1122334455667788ULL,0x8877665544332211ULL)==1);
  CHECK(word==0x8877665544332211ULL);
  CHECK(core_cx8(&word,0,5)==0 && word==0x8877665544332211ULL);
  CHECK(core_cwd(0x8000)==0xffff && core_cwd(0x7fff)==0);
  CHECK(core_enter()==37);
  core_flush(src+255);
  CHECK(munmap(a,2*page)==0 && munmap(b,2*page)==0);
  return 0;
}
