
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <signal.h>
#include <unistd.h>
#include <sys/mman.h>
extern uint32_t probe(void*,void*,uint64_t,uint64_t,uint32_t,uint32_t);
static void *afault,*bfault;
static void handler(int sig,siginfo_t *info,void *ctx) {
 (void)sig;(void)ctx;
 if(info->si_addr==bfault) {write(1,"B[0]\n",5);_Exit(20);}
 if(info->si_addr==afault) {write(1,"A[1]\n",5);_Exit(21);}
 _Exit(22);
}
int main(void) {
 long p=sysconf(_SC_PAGESIZE);
 char *a=mmap(0,p*2,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANON,-1,0);
 char *b=mmap(0,p*2,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANON,-1,0);
 if(a==MAP_FAILED||b==MAP_FAILED)return 90;
 afault=a+p;bfault=b+p;
 if(mprotect(afault,p,PROT_NONE)||mprotect(bfault,p,PROT_NONE))return 91;
 struct sigaction action={0};action.sa_sigaction=handler;action.sa_flags=SA_SIGINFO;
 sigaction(SIGSEGV,&action,0);sigaction(SIGBUS,&action,0);
 *(uint32_t*)(a+p-4)=7;
 return probe(a+p-4,b+p,16,0,0,3);
}
