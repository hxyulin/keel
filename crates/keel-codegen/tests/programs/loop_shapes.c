#include <stdint.h>
#include <string.h>
#include <stdlib.h>
#include <sys/mman.h>
#include <unistd.h>
#include <signal.h>
#include <setjmp.h>
extern void k_isort(int32_t *, uint64_t);
extern void k_matmul(int32_t *,int32_t *,int32_t *,int32_t);
extern uint32_t signed_dot32(const uint32_t *,const uint32_t *,int32_t,uint32_t);
extern uint64_t signed_dot64(const uint64_t *,const uint64_t *,int64_t,uint64_t);
extern int64_t div_loop(const int64_t *,const int64_t *,int64_t,int64_t);
static volatile sig_atomic_t fault_sig;
static sigjmp_buf fault_jmp;
static void *volatile fault_addr;
static void fault(int sig,siginfo_t *info,void *ctx) {
 (void)ctx; fault_sig=sig; fault_addr=info->si_addr; siglongjmp(fault_jmp,1);
}
static int cmp(const void *a,const void *b) {
 int32_t x=*(const int32_t *)a,y=*(const int32_t *)b;
 return (x>y)-(x<y);
}
int main(void) {
 k_matmul(0,0,0,INT32_MIN); k_matmul(0,0,0,-1); k_matmul(0,0,0,0);
 for(unsigned n=1;n<=12;n++) for(unsigned alias=0;alias<4;alias++) {
  int32_t got[512],want[512];
  for(unsigned i=0;i<512;i++) got[i]=(int32_t)(i*2654435761u+1234567u);
  memcpy(want,got,sizeof got);
  unsigned ai=2,bi=alias==1?2:160,ci=alias==2?2:alias==3?160:320;
  for(unsigned i=0;i<n;i++) for(unsigned j=0;j<n;j++) {
   uint32_t sum=0;
   for(unsigned k=0;k<n;k++) sum+=(uint32_t)want[ai+i*n+k]*(uint32_t)want[bi+k*n+j];
   want[ci+i*n+j]=(int32_t)sum;
  }
  k_matmul(got+ai,got+bi,got+ci,(int32_t)n);
  if(memcmp(got,want,sizeof got)) return 4;
 }
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
 for(unsigned n=0;n<=65;n++) {
  uint64_t *a=(uint64_t *)(mem+page*2)-n;
  for(unsigned i=0;i<n;i++) a[i]=UINT64_MAX-i*1234567u;
  uint64_t want64=UINT64_MAX-7;
  for(unsigned i=0;i<n;i++) want64+=a[i]*a[i];
  if(signed_dot64(a,a,n,UINT64_MAX-7)!=want64) return 5;
  uint32_t *b=(uint32_t *)(mem+page*2)-n;
  for(unsigned i=0;i<n;i++) b[i]=UINT32_MAX-i*1234567u;
  uint32_t want32=UINT32_MAX-7;
  for(unsigned i=0;i<n;i++) want32+=b[i]*b[i];
  if(signed_dot32(b,b,n,UINT32_MAX-7)!=want32) return 6;
 }
 if(signed_dot32(0,0,INT32_MIN,123)!=123 || signed_dot32(0,0,-1,456)!=456
  || signed_dot64(0,0,INT64_MIN,123)!=123 || signed_dot64(0,0,-1,456)!=456) return 7;
 // b[0] faults before a[1], although a[1] also touches a protected page.
 // Checking si_addr distinguishes a stream-major load reorder from iteration order.
 struct sigaction sa={0},oldsegv,oldbus;
 sa.sa_sigaction=fault; sa.sa_flags=SA_SIGINFO; sigemptyset(&sa.sa_mask);
 sigaction(SIGSEGV,&sa,&oldsegv); sigaction(SIGBUS,&sa,&oldbus);
 void *bad=mem;
 fault_addr=0;
 if(!sigsetjmp(fault_jmp,1)) { signed_dot32((uint32_t *)(mem+page*2)-1,bad,2,0); return 8; }
 if(fault_addr!=bad) return 9;
 fault_addr=0;
 if(!sigsetjmp(fault_jmp,1)) { signed_dot64((uint64_t *)(mem+page*2)-1,bad,2,0); return 10; }
 if(fault_addr!=bad) return 11;
 struct sigaction oldtrap,oldill;
 sigaction(SIGTRAP,&sa,&oldtrap); sigaction(SIGILL,&sa,&oldill);
 if(div_loop(0,0,0,0)!=0) return 12;
 int64_t min=INT64_MIN;
 for(int d=0;d<2;d++) {
  fault_sig=0;
  if(!sigsetjmp(fault_jmp,1)) { div_loop(&min,bad,2,d?-1:0); return 13; }
  if(fault_sig!=SIGTRAP && fault_sig!=SIGILL) return 14;
 }
 sigaction(SIGTRAP,&oldtrap,0); sigaction(SIGILL,&oldill,0);
 sigaction(SIGSEGV,&oldsegv,0); sigaction(SIGBUS,&oldbus,0);
 munmap(mem,page*3);
 return 0;
}
