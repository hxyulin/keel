/* Forced architectural instructions are independent of the C wrapper lowering. */
#define NUMERIC_ASM(TYPE, OPCODE, OUTPUT_CONSTRAINT, INPUT_CONSTRAINT) \
 __extension__({ \
  TYPE result=a; \
  __asm__ volatile(OPCODE " %1,%0":OUTPUT_CONSTRAINT(result):INPUT_CONSTRAINT(b):"memory"); \
  result; \
 })
#define CONVERSION(name, INPUT, OUTPUT, EXPR, INPUT_BYTES, OUTPUT_BYTES, KIND, ASSEMBLY) \
static void assembly##name(const void*input,const void*merge,void*out){ \
 INPUT b;OUTPUT a;__builtin_memcpy(&b,input,sizeof(b));__builtin_memcpy(&a,merge,sizeof(a)); \
 OUTPUT result=(ASSEMBLY);__builtin_memcpy(out,&result,sizeof(result)); \
}
#include "x86_numeric_cases.h"
#undef CONVERSION
#undef NUMERIC_ASM
