#include <immintrin.h>
#ifdef REFERENCE
#define FN_(name) reference##name
#else
#define FN_(name) keel##name
#endif
#define FN(name) FN_(name)
#define CONVERSION(name, INPUT, OUTPUT, EXPR, INPUT_BYTES, OUTPUT_BYTES, KIND, ASSEMBLY) \
OUTPUT FN(name)(INPUT b, OUTPUT a) { (void)a; return (EXPR); } \
void FN(mem##name)(const void*input,const void*merge,void*out) { \
  INPUT b; OUTPUT a; \
  __builtin_memcpy(&b,input,sizeof(b)); __builtin_memcpy(&a,merge,sizeof(a)); \
  OUTPUT result=(EXPR); __builtin_memcpy(out,&result,sizeof(result)); \
}
#include "x86_numeric_cases.h"
#undef CONVERSION
#ifndef REFERENCE
#define CONVERSION(name, INPUT, OUTPUT, EXPR, INPUT_BYTES, OUTPUT_BYTES, KIND, ASSEMBLY) \
void FN(dead##name)(const void*input,const void*merge,void*out) { \
  INPUT b; OUTPUT a; (void)out; \
  __builtin_memcpy(&b,input,sizeof(b)); __builtin_memcpy(&a,merge,sizeof(a)); \
  (void)a; (void)(EXPR); \
}
#include "x86_numeric_cases.h"
#undef CONVERSION
#endif
