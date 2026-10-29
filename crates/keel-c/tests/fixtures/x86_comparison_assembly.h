/* Forced instructions provide independent predicate and MXCSR evidence. */
#define SSE_COMPARE(TYPE, OPCODE, PREDICATE, SWAP, SCALAR_BYTES) \
 __extension__({ \
  TYPE r=(SWAP)?b:a,other=(SWAP)?a:b; \
  __asm__ volatile(OPCODE " $" #PREDICATE ",%1,%0":"+x"(r):"x"(other):"memory"); \
  if((SWAP)&&(SCALAR_BYTES)){__builtin_memcpy(&a,&r,SCALAR_BYTES);r=a;} \
  r; \
 })
#define AVX_COMPARE(TYPE, OPCODE, PREDICATE) \
 __extension__({ \
  TYPE r; \
  __asm__ volatile(OPCODE " $" #PREDICATE ",%2,%1,%0":"=x"(r):"x"(a),"x"(b):"memory"); \
  r; \
 })
static int comi_result(unsigned cf,unsigned zf,unsigned pf,unsigned condition){
 switch(condition){
  case 0:return !pf&&zf;
  case 1:return !pf&&cf;
  case 2:return !pf&&(cf||zf);
  case 3:return !cf&&!zf;
  case 4:return !cf;
  default:return pf||!zf;
 }
}
#define COMI_COMPARE(OPCODE, CONDITION) \
 __extension__({ \
  unsigned char cf,zf,pf; \
  __asm__ volatile(OPCODE " %4,%3;setb %0;sete %1;setp %2":"=qm"(cf),"=qm"(zf),"=qm"(pf):"x"(a),"x"(b):"memory","cc"); \
  comi_result(cf,zf,pf,CONDITION); \
 })
#define COMPARISON(name, INPUT, OUTPUT, EXPR, BYTES, BITS, COMI, ASSEMBLY) \
static void assembly_##name(const void*x,const void*y,void*out){ \
 INPUT a,b;__builtin_memcpy(&a,x,BYTES);__builtin_memcpy(&b,y,BYTES); \
 OUTPUT r=(ASSEMBLY);__builtin_memcpy(out,&r,sizeof(r)); \
}
#include "x86_comparison_cases.h"
#undef COMPARISON
#undef SSE_COMPARE
#undef AVX_COMPARE
#undef COMI_COMPARE
