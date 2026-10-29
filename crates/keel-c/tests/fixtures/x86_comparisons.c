#include <immintrin.h>
#ifdef REFERENCE
#define FN_(name) reference_##name
#else
#define FN_(name) keel_##name
#endif
#define FN(name) FN_(name)
#define COMPARISON(name, INPUT, OUTPUT, EXPR, BYTES, BITS, COMI, ASSEMBLY) \
OUTPUT FN(name)(INPUT a,INPUT b){return (EXPR);} \
void FN(mem_##name)(const void*x,const void*y,void*out){ \
 INPUT a,b;__builtin_memcpy(&a,x,BYTES);__builtin_memcpy(&b,y,BYTES); \
 OUTPUT r=(EXPR);__builtin_memcpy(out,&r,sizeof(r)); \
}
#include "x86_comparison_cases.h"
#undef COMPARISON
#ifndef REFERENCE
#define COMPARISON(name, INPUT, OUTPUT, EXPR, BYTES, BITS, COMI, ASSEMBLY) \
void FN(dead_##name)(const void*x,const void*y,void*out){ \
 INPUT a,b;(void)out;__builtin_memcpy(&a,x,BYTES);__builtin_memcpy(&b,y,BYTES);(void)(EXPR); \
}
#include "x86_comparison_cases.h"
#undef COMPARISON
#endif
#ifndef REFERENCE
static int evaluation_count;
static __m128 read_ps(void){evaluation_count++;return (__m128){0};}
static __m128d read_pd(void){evaluation_count++;return (__m128d){0};}
static __m256 read_ps256(void){evaluation_count++;return (__m256){0};}
static __m256d read_pd256(void){evaluation_count++;return (__m256d){0};}
int keel_compare_evaluations(void){
 evaluation_count=0;
 (void)_mm_cmp_ps(read_ps(),read_ps(),31);
 (void)_mm_cmp_pd(read_pd(),read_pd(),31);
 (void)_mm_cmp_ss(read_ps(),read_ps(),31);
 (void)_mm_cmp_sd(read_pd(),read_pd(),31);
 (void)_mm256_cmp_ps(read_ps256(),read_ps256(),31);
 (void)_mm256_cmp_pd(read_pd256(),read_pd256(),31);
 return evaluation_count;
}
#endif
