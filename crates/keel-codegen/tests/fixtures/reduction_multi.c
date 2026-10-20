#include <stdint.h>
#include <stdio.h>
extern void multi_32(void*,uint64_t,uint64_t,uint32_t,uint32_t,uint32_t,void*);
extern void multi_64(void*,uint64_t,uint64_t,uint64_t,uint64_t,uint64_t,void*);
static int multi_oracle(unsigned char *region,long page) {
 unsigned long checks=0;
#define CHECK(W) do { \
 for(unsigned n=0;n<=67;n++) { \
  uint##W##_t *p=(void*)(region+page-n*sizeof(*p)); \
  for(unsigned i=0;i<n;i++) p[i]=(uint##W##_t)(UINT64_C(0x9876fedc80007fff) ^ ((uint64_t)i*UINT64_C(0x123456781))); \
  for(unsigned start=0;start<=n+1;start++) for(unsigned k=0;k<3;k++) for(unsigned alias=0;alias<2;alias++) { \
   uint##W##_t a=(uint##W##_t)(-k),b=a+17,c=a+123,out[3],save[3]; \
   uint##W##_t x=a,y=b,z=c; \
   for(unsigned i=start;i<n;i++) { \
    uint##W##_t low=p[i] & 65535; \
    if(low & 32768) low |= (uint##W##_t)~UINT64_C(65535); \
    x+=p[i]; y+=low; z++; \
   } \
   uint##W##_t *dest=out; \
   if(alias && n>=3) { dest=p; for(unsigned j=0;j<3;j++)save[j]=p[j]; } \
   multi_##W(p,n,start,a,b,c,dest); \
   if(dest[0]!=x || dest[1]!=y || dest[2]!=z) { fprintf(stderr,"multi %d n=%u start=%u\n",W,n,start); return 1; } \
   if(dest==p)for(unsigned j=0;j<3;j++)p[j]=save[j]; \
   checks++; \
  } \
 } \
} while(0)
 CHECK(32); CHECK(64);
 printf("multi: %lu independent oracle comparisons\n",checks);
 return 0;
}
