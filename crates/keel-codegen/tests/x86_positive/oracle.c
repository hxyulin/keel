#define _GNU_SOURCE
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/wait.h>
#include <signal.h>
#include <unistd.h>
#include <stdlib.h>
#ifndef EXTERNAL
#define EXTERNAL 0
#endif
static int observer(const unsigned char *out,const unsigned char *src){return memcmp(out,src,8)||memcmp(out+8,src,8);}
typedef void (*Fn)(const void*,void*,uint64_t);
static unsigned char *region;
static size_t page;
static uintptr_t expected_fault;
static uint64_t random_state=0xd34db33f12345678ULL;
extern void positive_sum(const void*,void*);
static uint64_t random64(void){random_state^=random_state<<13;random_state^=random_state>>7;random_state^=random_state<<17;return random_state;}
static int setup(void){page=(size_t)sysconf(_SC_PAGESIZE);region=mmap(0,2*page,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);return region==MAP_FAILED||mprotect(region+page,page,PROT_NONE);}
static void fault(int sig,siginfo_t *info,void *context){(void)sig;(void)context;uintptr_t at=(uintptr_t)info->si_addr;_exit(at>=expected_fault&&at<expected_fault+page?0:90);}
static int fault_order(Fn run,unsigned bias,int live){
 void *out=mmap(0,page,PROT_NONE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);if(out==MAP_FAILED)return 1;
 pid_t child=fork();if(child<0)return 1;
 if(!child){struct sigaction action;memset(&action,0,sizeof action);action.sa_sigaction=fault;action.sa_flags=SA_SIGINFO;sigemptyset(&action.sa_mask);if(sigaction(SIGSEGV,&action,0))_exit(93);expected_fault=live?(uintptr_t)out:(uintptr_t)(region+page);run(region+page-bias,out,1);_exit(91);}
 int status=0;if(waitpid(child,&status,0)!=child)return 1;munmap(out,page);return !WIFEXITED(status)||WEXITSTATUS(status)!=0;
}
static void expected(const unsigned char *src,uint64_t *want){for(unsigned l=0;l<2;l++){int32_t x;memcpy(&x,src+4*l,4);want[l]=x>0?(uint32_t)x:0;}}
static int check(Fn run,unsigned bias,int live){
 const uint32_t edges[]={0,1,2,0x7fffffffU,0x80000000U,0xffffffffU,0x80000001U};
 for(unsigned n=0;n<49+4096;n++)for(unsigned unalign=0;unalign<4;unalign++){
  unsigned char *src=region+page-8-unalign,original[8],out[50],before[16],alias[64];uint64_t want[2];
  for(unsigned l=0;l<2;l++){uint32_t x=n<49?edges[(l?n%7:n/7)]:(uint32_t)random64();memcpy(src+4*l,&x,4);}
  memcpy(original,src,8);expected(original,want);memset(out,0xa5,sizeof out);memcpy(before,out+1,16);
  run(src-bias,out+1,1);
  if((EXTERNAL&&observer(out+33,original))||out[0]!=0xa5||out[49]!=0xa5||memcmp(out+1,want,16)||(live&&memcmp(out+17,before,16)))return 1;
  memset(alias,0xa5,sizeof alias);memcpy(alias+16,original,8);memcpy(before,alias+16,16);
  run(alias+16-bias,alias+16,1);
  if((EXTERNAL&&observer(alias+48,original))||memcmp(alias+16,want,16)||(live&&memcmp(alias+32,before,16)))return 1;
 }
 return fault_order(run,bias,live);
}
static int check_sum(void){
 for(unsigned n=0;n<4096;n++){
  uint32_t input[2]={(uint32_t)random64(),(uint32_t)random64()};uint64_t sum[2]={UINT64_MAX-n,random64()},want[2],delta[2];expected((const unsigned char*)input,delta);
  for(unsigned l=0;l<2;l++)want[l]=sum[l]+delta[l];positive_sum(input,sum);if(memcmp(sum,want,16))return 1;
 }
 return 0;
}
