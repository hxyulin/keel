#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <math.h>
#include <sys/mman.h>
#include <unistd.h>
extern void col64(const double*,double*,unsigned long), baseline64(const double*,double*,unsigned long);
extern void col32(const float*,float*,unsigned long), baseline32(const float*,float*,unsigned long);
static void fail(const char *why,unsigned long n,int mode) {fprintf(stderr,"%s: n=%lu mode=%d\n",why,n,mode);exit(1);}
#define TEST(T,BITS) \
static void test##BITS(unsigned long n,int mode) { \
 size_t pg=(size_t)sysconf(_SC_PAGESIZE),bytes=n*n*sizeof(T),rounded=(bytes+pg-1)/pg*pg; \
 char *mem=mmap(0,rounded+pg,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANON,-1,0); \
 char *dest=mmap(0,2*pg,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANON,-1,0); \
 if(mem==MAP_FAILED||dest==MAP_FAILED||mprotect(mem+rounded,pg,PROT_NONE)||mprotect(dest+pg,pg,PROT_NONE))exit(2); \
 T *a=(T*)(mem+rounded-bytes),*out=(T*)(dest+pg)-n,*expected=malloc((n+1)*sizeof(T)); \
 for(unsigned long i=0;i<n*n;i++)a[i]=(T)(i%3==0?(BITS==32?16777216.:9007199254740992.):i%3==1?1.:(BITS==32?-16777216.:-9007199254740992.)); \
 if(n&&mode)a[n*n/2]=mode==1?(T)INFINITY:mode==2?(T)-INFINITY:mode==3?(T)NAN:mode==4?(T)(BITS==32?0x1p-149:0x1p-1074):(T)-0.; \
 baseline##BITS(a,expected,n);col##BITS(a,out,n); \
 if(memcmp(out,expected,n*sizeof(T)))fail("baseline bits differ",n,mode); \
 for(unsigned long c=0;c<n;c++){volatile T sum=0;for(unsigned long r=0;r<n;r++)sum=sum+a[r*n+c];T value=sum;if(!(isnan(value)&&isnan(out[c]))&&memcmp(&value,out+c,sizeof(T)))fail("independent fold differs",n,mode);} \
 free(expected);munmap(mem,rounded+pg);munmap(dest,2*pg); \
} \
static void alias##BITS(unsigned long n,unsigned long input,unsigned long output) { \
 unsigned long count=n*n+n+4;T *a=malloc(count*sizeof(T)),*b=malloc(count*sizeof(T));if(!a||!b)exit(2); \
 for(unsigned long i=0;i<count;i++)a[i]=(T)((int)(i%19)-9);memcpy(b,a,count*sizeof(T)); \
 baseline##BITS(a+input,a+output,n);col##BITS(b+input,b+output,n); \
 if(memcmp(a,b,count*sizeof(T)))fail("overlap fallback differs",n,0); \
 free(a);free(b); \
}
TEST(double,64)
TEST(float,32)
static void check(void) {
 unsigned long sizes[]={0,1,127,128,129,135,143};
 for(unsigned k=0;k<sizeof(sizes)/sizeof(sizes[0]);k++)for(int mode=0;mode<6;mode++){test64(sizes[k],mode);test32(sizes[k],mode);}
 for(unsigned long n=128;n<=129;n++)for(int mode=0;mode<6;mode++){
  unsigned long in=mode==3?1:mode==5?n:0,out=mode==0?0:mode==1?1:mode==2?n*n-1:mode==4?n*n:0;
  alias64(n,in,out);alias32(n,in,out);
 }
 return;
}

static volatile double sink;
int main(int argc,char **argv) {
 if(argc==1||(argc==2&&!strcmp(argv[1],"check"))){check();return 0;}
 if(argc!=4)return 2;
 int kind=atoi(argv[1]);unsigned long n=strtoul(argv[2],0,10),repeats=strtoul(argv[3],0,10);
 if(kind<0||kind>1||!n||n>2048||!repeats||repeats>100000)return 2;
 size_t bytes=(kind?sizeof(float):sizeof(double));void *a=malloc(n*n*bytes),*out=malloc(n*bytes),*expected=malloc(n*bytes);
 if(!a||!out||!expected)return 2;
 uint32_t state=17;
 for(unsigned long i=0;i<n*n;i++){state=state*1664525u+1013904223u;double x=((int)(state%20001)-10000)/16.;if(kind)((float*)a)[i]=(float)x;else((double*)a)[i]=x;}
 if(kind){baseline32(a,expected,n);col32(a,out,n);}else{baseline64(a,expected,n);col64(a,out,n);}
 if(memcmp(out,expected,n*bytes))fail("timed result differs",n,kind);
 if(kind){for(unsigned long r=0;r<repeats;r++){col32(a,out,n);sink=((float*)out)[r%n];}}
 else{for(unsigned long r=0;r<repeats;r++){col64(a,out,n);sink=((double*)out)[r%n];}}
 printf("%a\n",kind?(double)((float*)out)[0]:((double*)out)[0]);free(a);free(out);free(expected);return 0;
}
