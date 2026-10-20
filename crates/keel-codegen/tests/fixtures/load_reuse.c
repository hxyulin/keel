/* Independent native oracle, with protected pages to identify the first fault. */
#include <stdint.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#include <signal.h>
#include <unistd.h>
#include <sys/mman.h>
#include <sys/wait.h>
extern uint64_t reuse(void *, void *, uint64_t);
extern uint64_t alias_write(void *, void *, uint64_t);
extern uint64_t call_barrier(void *);
extern uint64_t width(void *);
extern uint64_t probe(void *, void *, uint64_t);
extern uint64_t wide_probe(void *);
extern uint32_t narrow_probe(void *);
void edit(void *p) { uint64_t x; memcpy(&x,p,8); x += UINT64_C(0xfedcba9876543210); memcpy(p,&x,8); }
static void *first, *second;
static void fault(int sig, siginfo_t *info, void *ctx) {
 (void)sig; (void)ctx;
 _Exit(info->si_addr == first ? 31 : info->si_addr == second ? 32 : 33);
}
static void faults(void) {
 long page=sysconf(_SC_PAGESIZE);
 first=mmap(0,(size_t)page,PROT_NONE,MAP_PRIVATE|MAP_ANON,-1,0);
 second=mmap(0,(size_t)page,PROT_NONE,MAP_PRIVATE|MAP_ANON,-1,0);
 if(first==MAP_FAILED || second==MAP_FAILED) abort();
 unsigned char *edge=mmap(0,(size_t)page*2,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANON,-1,0);
 if(edge==MAP_FAILED || mprotect(edge+page,(size_t)page,PROT_NONE))abort();
 uint64_t valid=17;
 for(int mode=0;mode<5;mode++) {
  pid_t child=fork(); if(child<0) abort();
  if(child==0) {
   struct sigaction sa={0}; sa.sa_sigaction=fault; sa.sa_flags=SA_SIGINFO;
   sigaction(SIGSEGV,&sa,0); sigaction(SIGBUS,&sa,0);
   if(mode==0) probe(first,second,1);
   if(mode==1) probe(&valid,second,1);
   if(mode==2 && probe(first,second,0)!=0) _Exit(34);
   if(mode>=3) {
    first=edge+page; second=0;
    if(mode==3) wide_probe(edge+page-4);
    else narrow_probe(edge+page-4);
   }
   _Exit(mode==2 ? 0 : 35);
  }
  int status=0; if(waitpid(child,&status,0)!=child || !WIFEXITED(status) ||
   WEXITSTATUS(status)!=(mode==0||mode>=3?31:mode==1?32:0)) abort();
 }
 munmap(edge,(size_t)page*2);
 munmap(first,(size_t)page); munmap(second,(size_t)page);
}
int main(void) {
 uint64_t cases[]={0,1,UINT64_MAX,UINT64_C(0x8000000000000000),UINT64_C(0x7fffffffffffffff),UINT64_C(0x0123456789abcdef)};
 unsigned long checks=0;
 for(unsigned i=0;i<6;i++) for(unsigned j=0;j<6;j++) for(int off=0;off<16;off++) {
  unsigned char a[64],b[64];
  for(int k=0;k<64;k++) a[k]=b[k]=(unsigned char)(k*37+i*13);
  uint64_t x=cases[i], y=cases[j], got;
  memcpy(a+off,&x,8); memcpy(b+off,&x,8);
  got=reuse(a+off,a+32,j&1);
  if(got!=x) abort();
  if(j&1) {memcpy(&got,a+32,8);if(got!=x)abort();}
  if(probe(a+off,a+off,1)!=0) abort();
  uint32_t hi; memcpy(&hi,a+off+4,4);
  if(width(a+off)!=(x^(uint64_t)hi))abort();
  // Full and partial aliases plus adjacent/disjoint writes.
  for(int delta=-7;delta<=8;delta++) {
   for(int k=0;k<64;k++) a[k]=b[k]=(unsigned char)(k*19+j*7);
   memcpy(&x,b+16,8); memcpy(b+16+delta,&y,8); memcpy(&got,b+16,8);
   if(alias_write(a+16,a+16+delta,y)!=(x^got) || memcmp(a,b,64))abort();
   checks++;
  }
  memcpy(a+off,&x,8);
  if(call_barrier(a+off)!=(x^(x+UINT64_C(0xfedcba9876543210))))abort();
  checks+=5;
 }
 faults();
 printf("load-reuse oracle: %lu value/memory checks + 5 first-fault/width/untaken checks\n",checks);
 return 0;
}
