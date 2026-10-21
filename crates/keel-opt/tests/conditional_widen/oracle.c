#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>
#ifndef ORACLE_CASES
#define ORACLE_CASES 7
#endif
typedef uint64_t (*Fn)(const void *,uint64_t,uint64_t,uint64_t);
extern uint64_t positive(const void *,uint64_t,uint64_t,uint64_t);
extern uint64_t balance(const void *,uint64_t,uint64_t,uint64_t);
extern uint64_t wrap_signed(const void *,uint64_t,uint64_t,uint64_t);
extern uint64_t wrap_unsigned(const void *,uint64_t,uint64_t,uint64_t);
extern uint64_t nested(const void *,uint64_t,uint64_t,uint64_t);
extern uint64_t clamped(const void *,uint64_t,uint64_t,uint64_t);
extern uint64_t clamped_unsigned(const void *,uint64_t,uint64_t,uint64_t);
static uint64_t contribution(unsigned k,uint32_t x) {
 int32_t s; memcpy(&s,&x,4);
 if(k==0) return s>0?x:0;
 if(k==1) return s<0?(uint64_t)(int64_t)s:7;
 if(k==2) { x+=UINT32_C(2147483647); memcpy(&s,&x,4); return (uint64_t)(int64_t)s; }
 if(k==3) return x+UINT32_C(1);
 if(k==4) {uint32_t y=x+1; int32_t t; memcpy(&t,&y,4);return y<x?(uint64_t)(int64_t)t:7;}
 if(k==5) return s < -128 ? 0 : s > 127 ? 255 : (uint64_t)(s+128);
 return x>UINT32_C(0xffffff00)?UINT32_MAX:x;
}
int main(void) {
 size_t page=(size_t)sysconf(_SC_PAGESIZE);
 unsigned char *p=mmap(0,3*page,PROT_NONE,MAP_PRIVATE|MAP_ANON,-1,0);
 if(p==MAP_FAILED||mprotect(p+page,page,PROT_READ|PROT_WRITE)) return 2;
 unsigned char *end=p+2*page;
 Fn f[]={positive,balance
#if ORACLE_CASES > 2
 ,wrap_signed,wrap_unsigned,nested
#endif
#if ORACLE_CASES > 5
 ,clamped,clamped_unsigned
#endif
 };
 uint32_t edge[]={0,1,UINT32_MAX,INT32_MAX,UINT32_C(0x80000000),UINT32_C(0x80000001),
  (uint32_t)-129,(uint32_t)-128,(uint32_t)-127,126,127,128,UINT32_C(0xffffff00),UINT32_C(0xffffff01)};
 unsigned edges=sizeof(edge)/sizeof(edge[0]);
 uint64_t seeds[]={0,UINT64_MAX,UINT64_C(0x7fffffffffffffff),UINT64_C(0x8000000000000000)};
 unsigned checks=0;
 for(unsigned k=0;k<ORACLE_CASES;k++) for(unsigned n=0;n<97;n++)
 for(unsigned offset=0;offset<4;offset++) for(unsigned pattern=0;pattern<edges+2;pattern++) {
  unsigned char *a=end-n*4-offset;
  for(unsigned i=0;i<n;i++) {uint32_t x=pattern<edges?edge[pattern]:pattern==edges?edge[i%edges]:(uint32_t)(i*UINT32_C(0x9e3779b9)+17);memcpy(a+4*i,&x,4);}
  for(unsigned start=0;start<3;start++) for(unsigned seed=0;seed<4;seed++) {
   uint64_t expected=seeds[seed];
   for(unsigned i=start;i<n;i++) {uint32_t x;memcpy(&x,a+4*i,4);expected+=contribution(k,x);}
   uint64_t actual=f[k](a,n,start,seeds[seed]); checks++;
   if(actual!=expected) {fprintf(stderr,"k=%u n=%u offset=%u pattern=%u start=%u seed=%u got=%llu expected=%llu\n",k,n,offset,pattern,start,seed,(unsigned long long)actual,(unsigned long long)expected);return 1;}
  }
 }
 for(unsigned k=0;k<ORACLE_CASES;k++) {
  if(f[k](end,0,UINT64_MAX,19)!=19 || f[k](p,0,0,19)!=19 || f[k](end,UINT64_MAX,UINT64_MAX,19)!=19) return 3;
 }
 printf("{\"native_arm\":\"passed\",\"oracle_calls_per_configuration\":%u,\"configurations\":8,\"lengths\":[0,96],\"unaligned_offsets\":[0,3],\"guard_pages\":true,\"wrapping_seeds\":true}\n",checks+3*ORACLE_CASES);
 return 0;
}
