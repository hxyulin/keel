#include <stdint.h>
#include <stdio.h>
#define DECL(W,N,S) extern uint##W##_t cast_##W##_##N##_##S(void*,uint64_t,uint64_t,uint##W##_t);
#define BOTH(W,N) DECL(W,N,s) DECL(W,N,u)
BOTH(32,8) BOTH(32,16) BOTH(64,8) BOTH(64,16) BOTH(64,32)
static int casts_oracle(unsigned char *region,long page) {
 unsigned long checks=0;
#define CHECK(W,N,S,SIGNED) do { \
 for(unsigned n=0;n<=67;n++) { \
  uint##W##_t *p=(void*)(region+page-n*sizeof(*p)); \
  for(unsigned i=0;i<n;i++) { \
   uint64_t edges[]={0,1,(UINT64_C(1)<<(N-1))-1,UINT64_C(1)<<(N-1),(UINT64_C(1)<<N)-1,UINT64_MAX}; \
   p[i]=(uint##W##_t)(edges[i%6] ^ ((uint64_t)(i/6)<<N)); \
  } \
  for(unsigned start=0;start<=n+1;start++) for(unsigned k=0;k<3;k++) { \
   uint##W##_t seed=k==0?0:k==1?(uint##W##_t)-1:(uint##W##_t)(UINT64_C(1)<<(W-1)),want=seed; \
   for(unsigned i=start;i<n;i++) { \
    uint##W##_t low=p[i] & ((UINT64_C(1)<<N)-1); \
    if(SIGNED && (low & (UINT64_C(1)<<(N-1)))) low |= (uint##W##_t)~((UINT64_C(1)<<N)-1); \
    want+=low; \
   } \
   if(cast_##W##_##N##_##S(p,n,start,seed)!=want) { fprintf(stderr,"cast %d/%d/%s n=%u start=%u\n",W,N,#S,n,start); return 1; } \
   checks++; \
  } \
 } \
} while(0)
#define PAIR(W,N) CHECK(W,N,u,0); CHECK(W,N,s,1)
 PAIR(32,8); PAIR(32,16); PAIR(64,8); PAIR(64,16); PAIR(64,32);
 printf("casts: %lu independent oracle comparisons\n",checks);
 return 0;
}
