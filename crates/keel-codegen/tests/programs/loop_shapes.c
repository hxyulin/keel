#include <stdint.h>
#include <string.h>
#include <stdlib.h>
#include <sys/mman.h>
#include <unistd.h>
extern void k_isort(int32_t *, uint64_t);
static int cmp(const void *a,const void *b) {
 int32_t x=*(const int32_t *)a,y=*(const int32_t *)b;
 return (x>y)-(x<y);
}
int main(void) {
 k_isort(0,0); k_isort(0,1);
 for (unsigned n=0;n<=65;n++) for (unsigned mode=0;mode<8;mode++) {
  int32_t a[69],b[69];
  for(unsigned i=0;i<69;i++) a[i]=(int32_t)(i*2654435761u+mode*1234567u);
  if(mode==0) for(unsigned i=0;i<n;i++) a[i+2]=(int32_t)i;
  if(mode==1) for(unsigned i=0;i<n;i++) a[i+2]=-(int32_t)i;
  if(mode==2) for(unsigned i=0;i<n;i++) a[i+2]=INT32_MIN;
  if(mode==3) for(unsigned i=0;i<n;i++) a[i+2]=(i&1)?INT32_MAX:INT32_MIN;
  memcpy(b,a,sizeof a); qsort(b+2,n,4,cmp); k_isort(a+2,n);
  if(memcmp(a,b,sizeof a)) return 1;
 }
 size_t page=(size_t)sysconf(_SC_PAGESIZE);
 char *mem=mmap(0,page*3,PROT_NONE,MAP_PRIVATE|MAP_ANON,-1,0);
 if(mem==MAP_FAILED || mprotect(mem+page,page,PROT_READ|PROT_WRITE)) return 2;
 for(unsigned n=1;n<66;n++) for(unsigned end=0;end<2;end++) {
  int32_t *a=(int32_t *)(mem+page+(end?page-n*4:0));
  for(unsigned i=0;i<n;i++) a[i]=(int32_t)(n-i);
  k_isort(a,n);
  for(unsigned i=0;i<n;i++) if(a[i]!=(int32_t)(i+1)) return 3;
 }
 munmap(mem,page*3);
 return 0;
}
