#include <stdint.h>
#include <stdio.h>
extern uint64_t k_logic(void*,uint64_t);
static int logic_oracle(unsigned char *region,long page) {
 for(unsigned n=0;n<=67;n++) {
  uint64_t *p=(void*)(region+page-2*n*8);
  for(unsigned i=0;i<2*n;i++)p[i]=UINT64_C(0x0123456789abcdef)*((uint64_t)i+1);
  uint64_t want=0,mask=UINT64_C(0x00ff00ff00ff00ff);
  for(unsigned i=0;i<n;i++)want+=(p[i]&mask)+(p[n+i]^mask)+(p[i]|mask);
  if(k_logic(p,n)!=want) { fprintf(stderr,"logic n=%u\n",n); return 1; }
 }
 printf("logic notrap fixture: 68 independent oracle comparisons\n");
 return 0;
}
