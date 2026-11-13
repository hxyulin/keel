#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
typedef struct { uint8_t byte; void *ptr; } Pair;
typedef struct { uint64_t first,second; } WidePair;
extern WidePair duplicate(uint64_t);
__attribute__((naked)) Pair c_pair(uint64_t x) {
 // Deliberately poison every unused high bit of the narrow first result.
 __asm__("movq %rdi,%rdx\n movabsq $0xfedcba9876543210,%rax\n xorq %rax,%rdx\n movq %rdi,%rax\n xorq $0x91,%rax\n movabsq $0xffffffffffffff00,%rcx\n orq %rcx,%rax\n ret");
}
extern Pair pair_out(int8_t,void *),pair_call(uint64_t),pair_indirect(Pair (*)(uint64_t),uint64_t);
extern uint64_t ignore(uint64_t),pair_checksum(uint64_t);
#define PARAMS uint64_t *,uint64_t,uint64_t,uint64_t,uint64_t,uint64_t,uint64_t,uint64_t,uint64_t,uint64_t,uint64_t
extern void bank_named(PARAMS),bank_mixed(PARAMS),bank_from_fallback(PARAMS);
int main(void) {
 void (*wrappers[])(PARAMS)={bank_named,bank_mixed,bank_from_fallback};
 uint64_t cases[]={0,1,127,128,255,256,65535,65536,UINT64_C(0xabcdef0187654321),UINT64_MAX};
 for(unsigned k=0;k<sizeof(cases)/sizeof(*cases);++k) {
  uint64_t x=cases[k],a=x^UINT64_C(0x123456789abcdef0),b=x+3,c=x+17,d=x^UINT64_C(0x70123456789abcde);
  uint64_t expected[]={x,(uint8_t)a,(uint16_t)b,(uint32_t)c,d,(uint8_t)a,(uint16_t)b};
  for(unsigned j=0;j<3;++j){uint64_t out[7];wrappers[j](out,x,2,3,4,5,6,a,b,c,d);for(unsigned i=0;i<7;++i)if(out[i]!=expected[i]){fprintf(stderr,"bank %u result %u: %llx != %llx\n",j,i,(unsigned long long)out[i],(unsigned long long)expected[i]);abort();}}
  WidePair duplicated=duplicate(x);
  if(duplicated.first!=x||duplicated.second!=x)abort();
  Pair want={(uint8_t)(x^0x91),(void *)(uintptr_t)(x^UINT64_C(0xfedcba9876543210))},got=pair_call(x),ind=pair_indirect(c_pair,x),own=pair_out((int8_t)x,(void *)(uintptr_t)a);
  if(got.byte!=want.byte||got.ptr!=want.ptr||ind.byte!=want.byte||ind.ptr!=want.ptr||own.byte!=(uint8_t)x||own.ptr!=(void *)(uintptr_t)a||ignore(x)!=x||pair_checksum(x)!=((uint64_t)want.byte^(uintptr_t)want.ptr))abort();
 }
 return 0;
}
