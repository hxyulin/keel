#define _GNU_SOURCE
#ifndef KEEL_CROSS_SYNTAX
#include <stdint.h>
#include <stddef.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/wait.h>
#include <unistd.h>
#include <signal.h>
#else
typedef unsigned long long uint64_t; typedef __SIZE_TYPE__ size_t; typedef int pid_t;
extern void *stderr; extern int fprintf(void*,const char*,...); extern void exit(int); extern int puts(const char*); extern int printf(const char*,...); extern void free(void*); extern int posix_memalign(void**,size_t,size_t); extern void *memset(void*,int,size_t); extern void *memcpy(void*,const void*,size_t); extern int memcmp(const void*,const void*,size_t); extern pid_t fork(void); extern pid_t waitpid(pid_t,int*,int); extern void _exit(int);
extern long sysconf(int); extern void *mmap(void*,size_t,int,int,int,long); extern int mprotect(void*,size_t,int);
#define _SC_PAGESIZE 30
#define PROT_READ 1
#define PROT_WRITE 2
#define PROT_NONE 0
#define MAP_PRIVATE 2
#define MAP_ANONYMOUS 32
#define MAP_FAILED ((void*)-1)
#define WIFSIGNALED(s) (((s)&127)!=0 && ((s)&127)!=127)
#define WTERMSIG(s) ((s)&127)
#define SIGSEGV 11
#define SIGBUS 7
#endif
extern void save(void *,uint64_t);
static void cpuid(unsigned leaf,unsigned sub,unsigned *a,unsigned *b,unsigned *c,unsigned *d){__asm__ volatile("cpuid":"=a"(*a),"=b"(*b),"=c"(*c),"=d"(*d):"0"(leaf),"2"(sub));}
static void fail(int n){fprintf(stderr,"FAIL xstate %d\n",n);exit(n);}
static uint64_t rd64(const void *p){uint64_t x;memcpy(&x,p,8);return x;}
extern void restore(void *,uint64_t);
extern void oracle_save(void *,uint64_t);
extern void oracle_restore(void *,uint64_t);
extern void clang_save(void *,uint64_t);
extern void clang_restore(void *,uint64_t);
extern void probe(void (*)(void*,uint64_t),void*,uint64_t,const void*,void*,void*);
extern unsigned ordered(void*,uint64_t,unsigned);
extern unsigned raw_ordered(void*,uint64_t,const void*,void*);
static size_t n,yoff;
static unsigned faults;
static unsigned char *alloc_image(void){void *p;if(posix_memalign(&p,64,n+64))fail(83);memset(p,0,n+64);memset((char*)p+n,0xa5,64);return p;}
static void wr64(void *p,uint64_t x){memcpy(p,&x,8);}
static void seed(unsigned char *p,int full,unsigned mx,unsigned salt){memset(p,0,n);wr64(p+512,7);p[0]=0x7f;p[1]=3;p[4]=full?255:0;memcpy(p+24,&mx,4);for(unsigned r=0;r<16;r++)for(unsigned j=0;j<16;j++){p[160+r*16+j]=(unsigned char)(1+r*13+j*7+salt);p[yoff+r*16+j]=(unsigned char)(129+r*17+j*11+salt);}for(unsigned r=0;r<8;r++){wr64(p+32+r*16,0x8000000000000000ULL+(r+1)*0x1010101ULL+salt);p[40+r*16]=255;p[41+r*16]=63;}}
static void equal(const unsigned char *a,const unsigned char *b,size_t off,size_t count,int code){if(memcmp(a+off,b+off,count))fail(code);}
static void state_equal(const unsigned char *a,const unsigned char *b,unsigned mask,int full){if(mask&1){equal(a,b,0,5,20);if(a[6]!=b[6]||((a[7]^b[7])&7))fail(21);equal(a,b,8,16,21);if(full)for(unsigned r=0;r<8;r++)equal(a,b,32+r*16,10,22);}if(mask&6)equal(a,b,24,8,23);if(mask&2)equal(a,b,160,256,24);if(mask&4)equal(a,b,yoff,256,25);}
static void guards(const unsigned char *p){for(size_t j=n;j<n+64;j++)if(p[j]!=0xa5)fail(26);}
static void expected_fault(void (*op)(void*,uint64_t),void *p,uint64_t m){pid_t child=fork();if(child<0)fail(84);if(!child){op(p,m);_exit(0);}int st;if(waitpid(child,&st,0)!=child||!WIFSIGNALED(st)||(WTERMSIG(st)!=SIGSEGV&&WTERMSIG(st)!=SIGBUS))fail(27);faults++;}
extern void save_live0(void*,uint64_t,const void*,void*,uint64_t);
extern void save_live1(void*,uint64_t,const void*,void*,uint64_t);
extern void save_live2(void*,uint64_t,const void*,void*,uint64_t);
extern void save_live3(void*,uint64_t,const void*,void*,uint64_t);
extern void save_live4(void*,uint64_t,const void*,void*,uint64_t);
extern void save_live5(void*,uint64_t,const void*,void*,uint64_t);
extern void save_live6(void*,uint64_t,const void*,void*,uint64_t);
extern void save_live7(void*,uint64_t,const void*,void*,uint64_t);
extern void save_live8(void*,uint64_t,const void*,void*,uint64_t);
extern void save_live9(void*,uint64_t,const void*,void*,uint64_t);
extern void save_live10(void*,uint64_t,const void*,void*,uint64_t);
extern void save_live11(void*,uint64_t,const void*,void*,uint64_t);
extern void save_live12(void*,uint64_t,const void*,void*,uint64_t);
extern void save_live13(void*,uint64_t,const void*,void*,uint64_t);
extern void restore_live0(void*,uint64_t,const void*,void*,uint64_t);
extern void restore_live1(void*,uint64_t,const void*,void*,uint64_t);
extern void restore_live2(void*,uint64_t,const void*,void*,uint64_t);
extern void restore_live3(void*,uint64_t,const void*,void*,uint64_t);
extern void restore_live4(void*,uint64_t,const void*,void*,uint64_t);
extern void restore_live5(void*,uint64_t,const void*,void*,uint64_t);
extern void restore_live6(void*,uint64_t,const void*,void*,uint64_t);
extern void restore_live7(void*,uint64_t,const void*,void*,uint64_t);
extern void restore_live8(void*,uint64_t,const void*,void*,uint64_t);
extern void restore_live9(void*,uint64_t,const void*,void*,uint64_t);
extern void restore_live10(void*,uint64_t,const void*,void*,uint64_t);
extern void restore_live11(void*,uint64_t,const void*,void*,uint64_t);
extern void restore_live12(void*,uint64_t,const void*,void*,uint64_t);
extern void restore_live13(void*,uint64_t,const void*,void*,uint64_t);
static void (*const live[])(void*,uint64_t,const void*,void*,uint64_t)={save_live0,save_live1,save_live2,save_live3,save_live4,save_live5,save_live6,save_live7,save_live8,save_live9,save_live10,save_live11,save_live12,save_live13,restore_live0,restore_live1,restore_live2,restore_live3,restore_live4,restore_live5,restore_live6,restore_live7,restore_live8,restore_live9,restore_live10,restore_live11,restore_live12,restore_live13};
int main(void){
 unsigned a,b,c,d;cpuid(1,0,&a,&b,&c,&d);
 if((c&((1u<<26)|(1u<<27)|(1u<<28)))!=((1u<<26)|(1u<<27)|(1u<<28)))fail(80);
 unsigned lo,hi;__asm__ volatile("xgetbv":"=a"(lo),"=d"(hi):"c"(0));if((lo&7)!=7)fail(81);
 cpuid(13,0,&a,&b,&c,&d);n=b;cpuid(13,2,&a,&b,&c,&d);yoff=b;if(a!=256||yoff<576||yoff+256>n)fail(82);
 unsigned char *p=alloc_image(),*q=alloc_image(),*r=alloc_image(),*s=alloc_image(),*t=alloc_image(),*u=alloc_image();
 const unsigned masks[]={0,1,2,3,4,6,7};unsigned patterns=0,lives=0,raw=0;
 for(unsigned k=0;k<7;k++)for(unsigned variant=0;variant<2;variant++){
  unsigned m=masks[k];seed(s,1,variant?0xbf85:0x3f81,variant*37);memset(p,0,n);wr64(p+512,variant?5:0);memset(q,0,n);memset(r,0,n);
  probe(save,p,m,s,q,r);
  state_equal(p,q,m,1);state_equal(r,q,7,1);
  if(rd64(p+512)!=(uint64_t)((variant?5:0)|m))fail(10);
  // Unrequested components and all non-XSTATE_BV header bytes stay untouched.
  if(!(m&1)){for(unsigned j=0;j<24;j++)if(p[j])fail(11);for(unsigned j=32;j<160;j++)if(p[j])fail(11);}
  if(!(m&6))for(unsigned j=24;j<32;j++)if(p[j])fail(12);
  if(!(m&2))for(unsigned j=160;j<416;j++)if(p[j])fail(13);
  if(!(m&4))for(unsigned j=0;j<256;j++)if(p[yoff+j])fail(14);
  for(unsigned j=520;j<576;j++)if(p[j])fail(15);guards(p);
  // Native opcode oracle from the same known machine seed.
  memset(t,0,n);memset(u,0,n);probe(oracle_save,t,m,s,q,u);state_equal(p,t,m,1);if(rd64(t+512)!=(uint64_t)m)fail(16);
  memset(t,0,n);memset(u,0,n);probe(clang_save,t,m,s,q,u);state_equal(p,t,m,1);if(rd64(t+512)!=(uint64_t)m)fail(17);
  // Restore only compiler-compatible x87 stacks. Poisoned payload is ignored
  // for requested components whose XSTATE_BV bit is clear; MXCSR still loads.
  seed(s,0,0x1f80,0);seed(p,0,variant?0x9f85:0x5f83,71);if(variant)wr64(p+512,0);
  memset(q,0,n);memset(r,0,n);probe(restore,p,m,s,q,r);
  memset(t,0,n);memset(u,0,n);probe(oracle_restore,p,m,s,t,u);state_equal(r,u,7,0);
  memset(t,0,n);memset(u,0,n);probe(clang_restore,p,m,s,t,u);state_equal(r,u,7,0);
  if(m&1){if(r[4]||((r[3]>>3)&7))fail(30);if(variant&&(r[0]!=0x7f||r[1]!=3))fail(31);}
  if(m&6){if(memcmp(r+24,p+24,4))fail(32);}else equal(r,s,24,4,33);
  if(m&2){if(variant){for(unsigned j=160;j<416;j++)if(r[j])fail(34);}else equal(r,p,160,256,35);}else equal(r,s,160,256,36);
  if(m&4){if(variant){for(unsigned j=0;j<256;j++)if(r[yoff+j])fail(37);}else equal(r,p,yoff,256,38);}else equal(r,s,yoff,256,39);
  guards(p);guards(r);patterns+=2;
 }
 // Live scalar/vector SSA values and a GPR must survive both boundaries.
 uint64_t rng=1;seed(p,0,0x1f80,19);
 for(unsigned op=0;op<28;op++)for(unsigned k=0;k<128;k++)for(unsigned off=0;off<16;off++){
  unsigned char in[96],out[96];for(unsigned j=0;j<96;j++){rng=rng*6364136223846793005ULL+1;in[j]=rng>>32;out[j]=0xa5;}
  lives++;unsigned i=op%14,bytes=i==0?4:i==1?8:i<8?16:32;uint64_t integer=rng^0xdeadbeef12345678ULL;
  live[op](p,masks[k%7],in+off,out+off,integer);
  if(memcmp(in+off,out+off,bytes)||rd64(out+off+32)!=integer)fail(40);
  if(op<14){unsigned m=masks[k%7],found=0;for(unsigned reg=0;reg<16;reg++){int match=1;if(m&2)match&=!memcmp(p+160+reg*16,in+off,bytes<16?bytes:16);if((m&4)&&bytes==32)match&=!memcmp(p+yoff+reg*16,in+off+16,16);found|=match;}if(!found)fail(44);}
  for(unsigned j=0;j<96;j++)if(!(j>=off&&j<off+bytes)&&!(j>=off+32&&j<off+40)&&out[j]!=0xa5)fail(41);
 }
 // Explicit MXCSR/write/save/restore/read ordering, including masks 0 and 4.
 for(unsigned k=0;k<7;k++){memset(p,0,n);unsigned mx=k%2?0x9f85:0x3f81;if(ordered(p,masks[k],mx)!=mx)fail(42);if((masks[k]&6)&&memcmp(p+24,&mx,4))fail(43);}
 // Raw SSE arithmetic must use restored rounding and precede the following save.
 for(unsigned mask=2;mask<=4;mask+=2)for(unsigned rounding=0;rounding<4;rounding++){
  memset(p,0,n);unsigned mx=0x1f80|(rounding<<13),in=0x7f7fffff,out=0;memcpy(p+24,&mx,4);
  unsigned observed=raw_ordered(p,mask,&in,&out),expected=(rounding==1||rounding==3)?in:0x7f800000;
  if(out!=expected||observed!=(mx|0x28)||memcmp(p+24,&observed,4))fail(46);raw++;
 }
 expected_fault(save,p+1,7);expected_fault(restore,p+1,7);
 memset(p,0,n);wr64(p+512,1ULL<<63);expected_fault(restore,p,0);
 memset(p,0,n);wr64(p+520,1);expected_fault(restore,p,0);
 memset(p,0,n);p[528]=1;expected_fault(restore,p,0);
 memset(p,0,n);p[27]=128;expected_fault(restore,p,2);
 // A readable header followed by an inaccessible AVX region must fault.
 long page=sysconf(_SC_PAGESIZE);if(page<4096)fail(85);
 unsigned char *mapping=mmap(0,(size_t)page*3,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);if(mapping==MAP_FAILED)fail(86);
 if(mprotect(mapping+page*2,page,PROT_NONE))fail(87);
 unsigned char *edge=mapping+page*2-576;memset(edge,0,576);expected_fault(save,edge,4);wr64(edge+512,4);expected_fault(restore,edge,4);
 // Entire conservative CPUID size accessible, with a protected following page.
 size_t pages=(n+page-1)/page;unsigned char *bounded=mmap(0,(pages+1)*page,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);if(bounded==MAP_FAILED||mprotect(bounded+pages*page,page,PROT_NONE))fail(88);
 unsigned char *end=bounded+pages*page-((n+63)&~(size_t)63);memset(end,0,n);save(end,7);restore(end,7);
 if(patterns!=28||lives!=57344||faults!=8||raw!=8)fail(45);
 unsigned clean_mx=0x1f80;__asm__ volatile("fninit; ldmxcsr %0"::"m"(clean_mx):"memory");
 printf("PASS xstate masks=7 patterns=%u live=%u faults=%u raw=%u\n",patterns,lives,faults,raw);
 free(p);free(q);free(r);free(s);free(t);free(u);return 0;
}
