#include <stdint.h>
#include <string.h>
#include <math.h>
#include <sys/mman.h>
#include <unistd.h>

/* Bitwise comparison preserves signed zero; NaNs permit hardware payload choices.
   Volatile scalar operands and contraction disabled retain written FP order. */
#define SAME(X,Y) ((isnan(X)&&isnan(Y)) || !memcmp(&(X),&(Y),sizeof(X)))
#define CHECK(N,T,EXPR) \
static int test_##N(void) { \
 T actual[512], expected[512]; \
 const T values[]={0.0,-0.0,1.0,-1.0,INFINITY,-INFINITY,NAN,0x1p-149,0x1p-126,0x1p-1022,0x1p-1074,0x1.fffffep127,3.0,-7.0,0x1.000002p0,-0x1.fffffep-1,0x1.0000000000001p0,-0x1.fffffffffffffp-1}; \
 const unsigned count=sizeof(values)/sizeof(values[0]); \
 for(unsigned n=0;n<=65;n++)for(int delta=-5;delta<=5;delta++)for(unsigned alias=0;alias<3;alias++){ \
  for(unsigned j=0;j<512;j++)actual[j]=expected[j]=values[j%count]; \
  unsigned d=64,a=d+delta,b=alias==0?256:alias==1?d:d-delta; \
  for(unsigned i=0;i<n;i++){volatile T x=expected[a+i],y=expected[b+i];expected[d+i]=(EXPR);} \
  N(actual+d,actual+a,actual+b,n); \
  for(unsigned j=0;j<512;j++)if(!SAME(actual[j],expected[j]))return __LINE__; \
 } \
 long page=sysconf(_SC_PAGESIZE); \
 char *p=mmap(0,6*page,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0); \
 if(p==MAP_FAILED)return __LINE__; \
 for(unsigned j=1;j<6;j+=2)if(mprotect(p+j*page,page,PROT_NONE))return __LINE__; \
 for(unsigned n=0;n<=65;n++){ \
  T *d=(T*)(p+page)-n,*a=(T*)(p+3*page)-n,*b=(T*)(p+5*page)-n; \
  for(unsigned i=0;i<n;i++){a[i]=values[i%count];b[i]=values[(i+3)%count];volatile T x=a[i],y=b[i];expected[i]=(EXPR);} \
  N(d,a,b,n);for(unsigned i=0;i<n;i++)if(!SAME(d[i],expected[i]))return __LINE__; \
 } \
 return munmap(p,6*page)!=0; \
}
