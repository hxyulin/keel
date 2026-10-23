#define _GNU_SOURCE
#ifndef KEEL_CROSS_SYNTAX
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>
#else
typedef unsigned long long uint64_t;
typedef unsigned int uint32_t;
typedef unsigned short uint16_t;
typedef __SIZE_TYPE__ size_t;
extern int printf(const char*,...);extern void exit(int);extern void *memset(void*,int,size_t);extern void *memcpy(void*,const void*,size_t);extern int memcmp(const void*,const void*,size_t);
extern long sysconf(int);extern void *mmap(void*,size_t,int,int,int,long);extern int mprotect(void*,size_t,int);
#define _SC_PAGESIZE 30
#define PROT_READ 1
#define PROT_WRITE 2
#define PROT_NONE 0
#define MAP_PRIVATE 2
#define MAP_ANONYMOUS 32
#define MAP_FAILED ((void*)-1)
#endif
typedef uint32_t (*fn)(const void*,const void*,void*);
extern uint32_t run(fn,const void*,const void*,void*,uint16_t,uint16_t*,unsigned);
#define DECL(OP,M) extern uint32_t OP##_##M(const void*,const void*,void*);extern uint32_t ref_##OP##_##M(const void*,const void*,void*);
#define MODES(OP) DECL(OP,both) DECL(OP,value) DECL(OP,status) DECL(OP,neither)
MODES(fsin) MODES(fcos) MODES(f2xm1) MODES(fpatan) MODES(fyl2x) MODES(fyl2xp1) MODES(fscale) MODES(fprem) MODES(fprem1)
#define ROW(OP,M) {#OP "_" #M,OP##_##M,ref_##OP##_##M},
#define ROWS(OP) ROW(OP,both) ROW(OP,value) ROW(OP,status) ROW(OP,neither)
static struct {const char *name;fn generated,reference;} cases[]={ROWS(fsin) ROWS(fcos) ROWS(f2xm1) ROWS(fpatan) ROWS(fyl2x) ROWS(fyl2xp1) ROWS(fscale) ROWS(fprem) ROWS(fprem1)};
/* Raw payloads avoid both C compiler long double evaluation and libm. */
static const struct {uint64_t sig;uint16_t se;} inputs[]={
 {0,0},{0,0x8000},{0x8000000000000000ULL,0x3fff},{0x8000000000000000ULL,0xbfff},
 {0x8000000000000000ULL,0x3ffe},{0x8000000000000000ULL,0xbffe},
 {0x8000000000000001ULL,0x3fff},{0xffffffffffffffffULL,0x403d},
 {0x8000000000000000ULL,0x403e},{0x8000000000000001ULL,0x403e},
 {0x8000000000000000ULL,0xc03e},{0x8000000000000001ULL,0xc03e},
 {1,0},{0x7fffffffffffffffULL,0},{0x8000000000000000ULL,1},
 {0xffffffffffffffffULL,0x7ffe},{0x8000000000000000ULL,0x7fff},
 {0x8000000000000000ULL,0xffff},{0xc000000000001234ULL,0x7fff},
 {0x8000000000001234ULL,0x7fff},{0x4000000000000000ULL,0x3fff},
 {0x8000000000000000ULL,0},{0xaaaaaaaaaaaaaaaaULL,0x4002},
 /* Extra restricted-domain probes: +/-1/4 and the last representable value
  * inside each FYL2XP1 bound, followed by its first outside neighbor. */
 {0x8000000000000000ULL,0x3ffd},{0x8000000000000000ULL,0xbffd},
 {0x95f619980c4336f7ULL,0x3ffd},{0x95f619980c4336f8ULL,0x3ffd},
 {0x95f619980c4336f7ULL,0xbffd},{0x95f619980c4336f8ULL,0xbffd}
};
static void payload(unsigned char *p,unsigned i){memcpy(p,&inputs[i].sig,8);memcpy(p+8,&inputs[i].se,2);}
/* Domain comparisons use integer payload ordering, never host FP arithmetic.
 * The conservative common FYL2XP1 interval is |x| < 1-sqrt(1/2).
 * At exponent -2 its greatest representable in-range significand is
 * floor((1-sqrt(1/2))*2^65) = 0x95f619980c4336f7. This also lies inside
 * AMD's wider positive interval. Unclassified NaN/unsupported inputs to
 * these two restricted opcodes are diagnostics, not numerical equality gates.
 */
static int finite(unsigned a){
 unsigned e=inputs[a].se&0x7fff;
 return e!=0x7fff && (!e || (inputs[a].sig>>63));
}
static int magnitude_le(unsigned a,unsigned e,uint64_t sig){
 unsigned ae=inputs[a].se&0x7fff;
 return ae<e || (ae==e && inputs[a].sig<=sig);
}
static int numeric_domain(unsigned op,unsigned a){
 if(op==2)return finite(a)&&magnitude_le(a,0x3fff,0x8000000000000000ULL);
 if(op==5)return finite(a)&&magnitude_le(a,0x3ffd,0x95f619980c4336f7ULL);
 return 1;
}
/* B/ES/SF/TOP are structural. Numeric exception flags are checked only in
 * defined domains. C0/C2/C3 on arithmetic/transcendentals are not assumed
 * defined. C1 rounding is gated only where a precision exception makes its
 * direction meaningful. FPREM quotient bits require a completed reduction.
 */
static unsigned status_mask(unsigned op,unsigned a,unsigned b,unsigned sw){
 unsigned mask=0xb8c0;
 if(!numeric_domain(op,a))return mask;
 mask|=0x003f;
 int operands=finite(a)&&(op<3||finite(b));
 if(op<2 && finite(a)){
  mask|=0x0400;
  if(!(sw&0x0400) && (sw&0x20) && !(sw&5))mask|=0x0200;
 }else if(op>=7){
  if(operands && inputs[b].sig && !(sw&1)){
   mask|=0x0400;
   if(!(sw&0x0400))mask|=0x4300;
  }
 }else if(operands && (sw&0x20) && !(sw&5))mask|=0x0200;
 return mask;
}
static unsigned checks,exact_payloads,domain_diagnostics,raw_status_diagnostics;
static unsigned payload_differences,status_differences;
static int guards_equal(const unsigned char *g,const unsigned char *r,const unsigned char *out,int writes){
 for(unsigned i=0;i<48;i++){
  int payload=0;
  if(writes)for(unsigned j=0;j<10;j++)if(g+i==out+j)payload=1;
  if(!payload && g[i]!=r[i])return 0;
 }
 return 1;
}
static void state_check(unsigned c,unsigned a,unsigned b,unsigned cw,unsigned top,
                        uint32_t actual,uint32_t expected,const uint16_t *gs,const uint16_t *rs){
 unsigned op=c/4;
 int status=c%4==0||c%4==2;
 unsigned mask=status_mask(op,a,b,expected);
 unsigned post_mask=0xb8c0|(numeric_domain(op,a)?0x3f:0);
 unsigned snapshot_top=((top-(op>=6?2:1))&7)<<11;
 if((status && ((actual>>16)||((actual^expected)&mask)||(actual&0x3800)!=snapshot_top)) ||
    (!status && actual!=0) || ((gs[0]^rs[0])&post_mask) ||
    gs[1]!=cw || rs[1]!=cw || gs[2] || rs[2] ||
    (gs[0]&0x3800)!=(top<<11) || (rs[0]&0x3800)!=(top<<11)){
  printf("FAIL status %s a=%u b=%u cw=%04x top=%u mask=%04x expected=%08x actual=%08x post=%04x/%04x tag=%x/%x\n",
   cases[c].name,a,b,cw,top,mask,expected,actual,rs[0],gs[0],rs[2],gs[2]);exit(1);
 }
 raw_status_diagnostics++;
 if(actual!=expected||gs[0]!=rs[0])status_differences++;
}
static void check(unsigned c,unsigned a,unsigned b,unsigned cw,unsigned top,unsigned alias,int guarded){
 unsigned char ga[48],gb[48],ra[48],rb[48],go[48],ro[48];
 memset(ga,0xa5,48);memset(gb,0xa5,48);memset(ra,0xa5,48);memset(rb,0xa5,48);memset(go,0xa5,48);memset(ro,0xa5,48);
 unsigned char *g0=ga+9,*g1=gb+9,*gout=go+9,*r0=ra+9,*r1=rb+9,*rout=ro+9;
 if(guarded){
  /* All three pointers end exactly at a protected page boundary. */
  static unsigned char *pages[3];static long ps;
  if(!ps){ps=sysconf(_SC_PAGESIZE);if(ps<=0)exit(90);for(int i=0;i<3;i++){pages[i]=mmap(0,ps*2,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);if(pages[i]==MAP_FAILED||mprotect(pages[i]+ps,ps,PROT_NONE))exit(91);}}
  g0=pages[0]+ps-10;g1=pages[1]+ps-10;gout=pages[2]+ps-10;memset(gout,0xa5,10);
 }
 payload(g0,a);payload(g1,b);payload(r0,a);payload(r1,b);
 if(alias==1){gout=g0;rout=r0;}if(alias==2){gout=g1;rout=r1;}
 uint16_t gs[3],rs[3];
 uint32_t expected=run(cases[c].reference,r0,r1,rout,cw,rs,top);
 uint32_t actual=run(cases[c].generated,g0,g1,gout,cw,gs,top);
 state_check(c,a,b,cw,top,actual,expected,gs,rs);
 int writes=c%4<2,defined=numeric_domain(c/4,a);
 int difference=memcmp(gout,rout,10)!=0;
 if(writes && defined)exact_payloads++;
 if(writes && !defined){domain_diagnostics++;if(difference)payload_differences++;}
 if(difference && (!writes||defined)){
  printf("FAIL payload %s a=%u b=%u cw=%04x top=%u alias=%u guard=%d\n",cases[c].name,a,b,cw,top,alias,guarded);
  printf("payload expected/actual:");for(int i=9;i>=0;i--)printf(" %02x/%02x",rout[i],gout[i]);printf("\n");exit(1);
 }
 /* Always compare non-output bytes, even when result bits are undefined.
  * Mask only the ten-byte written range, including output/input aliases. */
 if(!guarded && (!guards_equal(ga,ra,gout,writes)||!guards_equal(gb,rb,gout,writes)||!guards_equal(go,ro,gout,writes)))exit(92);
 checks++;
}
static void unused_address(unsigned c,unsigned cw,unsigned top){
 unsigned char a[10],b[10];payload(a,2);payload(b,4);
 uint16_t gs[3],rs[3];
 uint32_t expected=run(cases[c].reference,a,b,(void*)1,cw,rs,top);
 uint32_t actual=run(cases[c].generated,a,b,(void*)1,cw,gs,top);
 state_check(c,2,4,cw,top,actual,expected,gs,rs);
 checks++;
}
int main(void){
 const unsigned n=23; /* Retain the original complete 23-by-23 matrix. */
 for(unsigned top=0;top<8;top++)for(unsigned pc=0;pc<3;pc++)for(unsigned rc=0;rc<4;rc++){
  unsigned cw=0x7f|((pc==0?0:pc==1?2:3)<<8)|(rc<<10);
  for(unsigned c=0;c<36;c++)if(c%4>=2)unused_address(c,cw,top);
  for(unsigned op=2;op<=5;op+=3)for(unsigned mode=0;mode<4;mode++)
   for(unsigned a=23;a<sizeof(inputs)/sizeof(inputs[0]);a++){
    unsigned c=op*4+mode;
    check(c,a,2,cw,top,0,0);check(c,a,2,cw,top,1,0);
    check(c,a,2,cw,top,2,0);check(c,a,2,cw,top,0,1);
   }
  for(unsigned c=0;c<36;c++)for(unsigned a=0;a<n;a++){
   unsigned limit=c/4<3?1:n;
   for(unsigned b=0;b<limit;b++)check(c,a,b,cw,top,0,0);
   check(c,a,(a+7)%n,cw,top,1,0);check(c,a,(a+11)%n,cw,top,2,0);check(c,a,(a+13)%n,cw,top,0,1);
  }
 }
 printf("PASS x87-numerical checks=%u exact_payloads=%u domain_diagnostics=%u raw_status_diagnostics=%u\n",checks,exact_payloads,domain_diagnostics,raw_status_diagnostics);
 printf("DIAGNOSTICS undefined_payload_differences=%u raw_status_differences=%u\n",payload_differences,status_differences);return 0;
}
