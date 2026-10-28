#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>
extern void copy(void *, const void *, uint64_t);
extern void move(void *, const void *, uint64_t);
extern void fill(void *, unsigned char, uint64_t);
extern void copy31(void *, const void *);
#define CHECK(x) do { if (!(x)) return __LINE__; } while (0)
int main(void) {
 unsigned char src[512], a[512], b[512];
 for (unsigned i=0;i<512;i++) src[i]=(unsigned char)(i*73+19);
 for (unsigned n=0;n<=129;n++) for(unsigned off=0;off<8;off++) {
  memset(a,93,512); memset(b,93,512);
  copy(a+off,src+3,n); memcpy(b+off,src+3,n); CHECK(!memcmp(a,b,512));
  copy(a+off,a+off,n); CHECK(!memcmp(a,b,512));
  fill(a+off,217,n); memset(b+off,217,n); CHECK(!memcmp(a,b,512));
  for(int delta=-17;delta<=17;delta++) {
   memcpy(a,src,512); memcpy(b,src,512);
   move(a+64+delta,a+64,n); memmove(b+64+delta,b+64,n); CHECK(!memcmp(a,b,512));
  }
 }
 copy(NULL,NULL,0); move(NULL,NULL,0); fill(NULL,255,0);
 size_t page=(size_t)sysconf(_SC_PAGESIZE);
 unsigned char *s=mmap(NULL,page*2,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANON,-1,0);
 unsigned char *d=mmap(NULL,page*2,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANON,-1,0);
 CHECK(s!=MAP_FAILED && d!=MAP_FAILED);
 CHECK(!mprotect(s+page,page,PROT_NONE) && !mprotect(d+page,page,PROT_NONE));
 for(unsigned n=0;n<=129;n++) {
  memcpy(s+page-n,src,n); copy(d+page-n,s+page-n,n); CHECK(!memcmp(d+page-n,src,n));
  fill(d+page-n,173,n); for(unsigned i=0;i<n;i++) CHECK(d[page-n+i]==173);
 }
 memcpy(s+page-31,src,31); copy31(d+page-31,s+page-31); CHECK(!memcmp(d+page-31,src,31));
 munmap(s,page*2); munmap(d,page*2);
 puts("memory intrinsic oracle passed"); return 0;
}
