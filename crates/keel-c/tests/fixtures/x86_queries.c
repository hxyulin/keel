#include <immintrin.h>
#ifdef REFERENCE
#define FN_(x) reference_##x
#define FN(x) FN_(x)
#else
#define FN(x) x
#endif
void FN(query_0)(unsigned char *out,const unsigned char *p,const unsigned char *q){ __m128i a,b; __builtin_memcpy(&a,p,16); __builtin_memcpy(&b,q,16); int v=_mm_testz_si128(a,b); __builtin_memcpy(out,&v,4); }
void FN(query_1)(unsigned char *out,const unsigned char *p,const unsigned char *q){ __m128i a,b; __builtin_memcpy(&a,p,16); __builtin_memcpy(&b,q,16); int v=_mm_testc_si128(a,b); __builtin_memcpy(out,&v,4); }
void FN(query_2)(unsigned char *out,const unsigned char *p,const unsigned char *q){ __m128i a,b; __builtin_memcpy(&a,p,16); __builtin_memcpy(&b,q,16); int v=_mm_testnzc_si128(a,b); __builtin_memcpy(out,&v,4); }
void FN(query_3)(unsigned char *out,const unsigned char *p,const unsigned char *q){ __m128 a,b; __builtin_memcpy(&a,p,16); __builtin_memcpy(&b,q,16); int v=_mm_testz_ps(a,b); __builtin_memcpy(out,&v,4); }
void FN(query_4)(unsigned char *out,const unsigned char *p,const unsigned char *q){ __m128 a,b; __builtin_memcpy(&a,p,16); __builtin_memcpy(&b,q,16); int v=_mm_testc_ps(a,b); __builtin_memcpy(out,&v,4); }
void FN(query_5)(unsigned char *out,const unsigned char *p,const unsigned char *q){ __m128 a,b; __builtin_memcpy(&a,p,16); __builtin_memcpy(&b,q,16); int v=_mm_testnzc_ps(a,b); __builtin_memcpy(out,&v,4); }
void FN(query_6)(unsigned char *out,const unsigned char *p,const unsigned char *q){ __m128d a,b; __builtin_memcpy(&a,p,16); __builtin_memcpy(&b,q,16); int v=_mm_testz_pd(a,b); __builtin_memcpy(out,&v,4); }
void FN(query_7)(unsigned char *out,const unsigned char *p,const unsigned char *q){ __m128d a,b; __builtin_memcpy(&a,p,16); __builtin_memcpy(&b,q,16); int v=_mm_testc_pd(a,b); __builtin_memcpy(out,&v,4); }
void FN(query_8)(unsigned char *out,const unsigned char *p,const unsigned char *q){ __m128d a,b; __builtin_memcpy(&a,p,16); __builtin_memcpy(&b,q,16); int v=_mm_testnzc_pd(a,b); __builtin_memcpy(out,&v,4); }
void FN(query_9)(unsigned char *out,const unsigned char *p,const unsigned char *q){ __m256i a,b; __builtin_memcpy(&a,p,32); __builtin_memcpy(&b,q,32); int v=_mm256_testz_si256(a,b); __builtin_memcpy(out,&v,4); }
void FN(query_10)(unsigned char *out,const unsigned char *p,const unsigned char *q){ __m256i a,b; __builtin_memcpy(&a,p,32); __builtin_memcpy(&b,q,32); int v=_mm256_testc_si256(a,b); __builtin_memcpy(out,&v,4); }
void FN(query_11)(unsigned char *out,const unsigned char *p,const unsigned char *q){ __m256i a,b; __builtin_memcpy(&a,p,32); __builtin_memcpy(&b,q,32); int v=_mm256_testnzc_si256(a,b); __builtin_memcpy(out,&v,4); }
void FN(query_12)(unsigned char *out,const unsigned char *p,const unsigned char *q){ __m256 a,b; __builtin_memcpy(&a,p,32); __builtin_memcpy(&b,q,32); int v=_mm256_testz_ps(a,b); __builtin_memcpy(out,&v,4); }
void FN(query_13)(unsigned char *out,const unsigned char *p,const unsigned char *q){ __m256 a,b; __builtin_memcpy(&a,p,32); __builtin_memcpy(&b,q,32); int v=_mm256_testc_ps(a,b); __builtin_memcpy(out,&v,4); }
void FN(query_14)(unsigned char *out,const unsigned char *p,const unsigned char *q){ __m256 a,b; __builtin_memcpy(&a,p,32); __builtin_memcpy(&b,q,32); int v=_mm256_testnzc_ps(a,b); __builtin_memcpy(out,&v,4); }
void FN(query_15)(unsigned char *out,const unsigned char *p,const unsigned char *q){ __m256d a,b; __builtin_memcpy(&a,p,32); __builtin_memcpy(&b,q,32); int v=_mm256_testz_pd(a,b); __builtin_memcpy(out,&v,4); }
void FN(query_16)(unsigned char *out,const unsigned char *p,const unsigned char *q){ __m256d a,b; __builtin_memcpy(&a,p,32); __builtin_memcpy(&b,q,32); int v=_mm256_testc_pd(a,b); __builtin_memcpy(out,&v,4); }
void FN(query_17)(unsigned char *out,const unsigned char *p,const unsigned char *q){ __m256d a,b; __builtin_memcpy(&a,p,32); __builtin_memcpy(&b,q,32); int v=_mm256_testnzc_pd(a,b); __builtin_memcpy(out,&v,4); }
void FN(query_18)(unsigned char *out,const unsigned char *p,const unsigned char *q){ __m128i a,b; __builtin_memcpy(&a,p,16); __builtin_memcpy(&b,q,16); __m128i v=_mm_minpos_epu16(a); __builtin_memcpy(out,&v,16); }
void FN(query_19)(unsigned char *out,const unsigned char *p,const unsigned char *q){ __m128i a,b; __builtin_memcpy(&a,p,16); __builtin_memcpy(&b,q,16); __m128i v=_mm_mpsadbw_epu8(a,b,0); __builtin_memcpy(out,&v,16); }
void FN(query_20)(unsigned char *out,const unsigned char *p,const unsigned char *q){ __m128i a,b; __builtin_memcpy(&a,p,16); __builtin_memcpy(&b,q,16); __m128i v=_mm_mpsadbw_epu8(a,b,1); __builtin_memcpy(out,&v,16); }
void FN(query_21)(unsigned char *out,const unsigned char *p,const unsigned char *q){ __m128i a,b; __builtin_memcpy(&a,p,16); __builtin_memcpy(&b,q,16); __m128i v=_mm_mpsadbw_epu8(a,b,4); __builtin_memcpy(out,&v,16); }
void FN(query_22)(unsigned char *out,const unsigned char *p,const unsigned char *q){ __m128i a,b; __builtin_memcpy(&a,p,16); __builtin_memcpy(&b,q,16); __m128i v=_mm_mpsadbw_epu8(a,b,7); __builtin_memcpy(out,&v,16); }
void FN(query_23)(unsigned char *out,const unsigned char *p,const unsigned char *q){ __m128i a,b; __builtin_memcpy(&a,p,16); __builtin_memcpy(&b,q,16); __m128i v=_mm_mpsadbw_epu8(a,b,24); __builtin_memcpy(out,&v,16); }
void FN(query_24)(unsigned char *out,const unsigned char *p,const unsigned char *q){ __m128i a,b; __builtin_memcpy(&a,p,16); __builtin_memcpy(&b,q,16); __m128i v=_mm_mpsadbw_epu8(a,b,63); __builtin_memcpy(out,&v,16); }
void FN(query_25)(unsigned char *out,const unsigned char *p,const unsigned char *q){ __m128i a,b; __builtin_memcpy(&a,p,16); __builtin_memcpy(&b,q,16); __m128i v=_mm_mpsadbw_epu8(a,b,255); __builtin_memcpy(out,&v,16); }
void FN(query_26)(unsigned char *out,const unsigned char *p,const unsigned char *q){ __m256i a,b; __builtin_memcpy(&a,p,32); __builtin_memcpy(&b,q,32); __m256i v=_mm256_mpsadbw_epu8(a,b,0); __builtin_memcpy(out,&v,32); }
void FN(query_27)(unsigned char *out,const unsigned char *p,const unsigned char *q){ __m256i a,b; __builtin_memcpy(&a,p,32); __builtin_memcpy(&b,q,32); __m256i v=_mm256_mpsadbw_epu8(a,b,1); __builtin_memcpy(out,&v,32); }
void FN(query_28)(unsigned char *out,const unsigned char *p,const unsigned char *q){ __m256i a,b; __builtin_memcpy(&a,p,32); __builtin_memcpy(&b,q,32); __m256i v=_mm256_mpsadbw_epu8(a,b,4); __builtin_memcpy(out,&v,32); }
void FN(query_29)(unsigned char *out,const unsigned char *p,const unsigned char *q){ __m256i a,b; __builtin_memcpy(&a,p,32); __builtin_memcpy(&b,q,32); __m256i v=_mm256_mpsadbw_epu8(a,b,7); __builtin_memcpy(out,&v,32); }
void FN(query_30)(unsigned char *out,const unsigned char *p,const unsigned char *q){ __m256i a,b; __builtin_memcpy(&a,p,32); __builtin_memcpy(&b,q,32); __m256i v=_mm256_mpsadbw_epu8(a,b,24); __builtin_memcpy(out,&v,32); }
void FN(query_31)(unsigned char *out,const unsigned char *p,const unsigned char *q){ __m256i a,b; __builtin_memcpy(&a,p,32); __builtin_memcpy(&b,q,32); __m256i v=_mm256_mpsadbw_epu8(a,b,63); __builtin_memcpy(out,&v,32); }
void FN(query_32)(unsigned char *out,const unsigned char *p,const unsigned char *q){ __m256i a,b; __builtin_memcpy(&a,p,32); __builtin_memcpy(&b,q,32); __m256i v=_mm256_mpsadbw_epu8(a,b,255); __builtin_memcpy(out,&v,32); }
void FN(query_33)(unsigned char *out,const unsigned char *p,const unsigned char *q){ __m128 a,b; __builtin_memcpy(&a,p,16); __builtin_memcpy(&b,q,16); __m128 v=_mm_dp_ps(a,b,0); __builtin_memcpy(out,&v,16); }
void FN(query_34)(unsigned char *out,const unsigned char *p,const unsigned char *q){ __m128 a,b; __builtin_memcpy(&a,p,16); __builtin_memcpy(&b,q,16); __m128 v=_mm_dp_ps(a,b,1); __builtin_memcpy(out,&v,16); }
void FN(query_35)(unsigned char *out,const unsigned char *p,const unsigned char *q){ __m128 a,b; __builtin_memcpy(&a,p,16); __builtin_memcpy(&b,q,16); __m128 v=_mm_dp_ps(a,b,18); __builtin_memcpy(out,&v,16); }
void FN(query_36)(unsigned char *out,const unsigned char *p,const unsigned char *q){ __m128 a,b; __builtin_memcpy(&a,p,16); __builtin_memcpy(&b,q,16); __m128 v=_mm_dp_ps(a,b,127); __builtin_memcpy(out,&v,16); }
void FN(query_37)(unsigned char *out,const unsigned char *p,const unsigned char *q){ __m128 a,b; __builtin_memcpy(&a,p,16); __builtin_memcpy(&b,q,16); __m128 v=_mm_dp_ps(a,b,241); __builtin_memcpy(out,&v,16); }
void FN(query_38)(unsigned char *out,const unsigned char *p,const unsigned char *q){ __m128 a,b; __builtin_memcpy(&a,p,16); __builtin_memcpy(&b,q,16); __m128 v=_mm_dp_ps(a,b,255); __builtin_memcpy(out,&v,16); }
void FN(query_39)(unsigned char *out,const unsigned char *p,const unsigned char *q){ __m128d a,b; __builtin_memcpy(&a,p,16); __builtin_memcpy(&b,q,16); __m128d v=_mm_dp_pd(a,b,0); __builtin_memcpy(out,&v,16); }
void FN(query_40)(unsigned char *out,const unsigned char *p,const unsigned char *q){ __m128d a,b; __builtin_memcpy(&a,p,16); __builtin_memcpy(&b,q,16); __m128d v=_mm_dp_pd(a,b,1); __builtin_memcpy(out,&v,16); }
void FN(query_41)(unsigned char *out,const unsigned char *p,const unsigned char *q){ __m128d a,b; __builtin_memcpy(&a,p,16); __builtin_memcpy(&b,q,16); __m128d v=_mm_dp_pd(a,b,18); __builtin_memcpy(out,&v,16); }
void FN(query_42)(unsigned char *out,const unsigned char *p,const unsigned char *q){ __m128d a,b; __builtin_memcpy(&a,p,16); __builtin_memcpy(&b,q,16); __m128d v=_mm_dp_pd(a,b,127); __builtin_memcpy(out,&v,16); }
void FN(query_43)(unsigned char *out,const unsigned char *p,const unsigned char *q){ __m128d a,b; __builtin_memcpy(&a,p,16); __builtin_memcpy(&b,q,16); __m128d v=_mm_dp_pd(a,b,241); __builtin_memcpy(out,&v,16); }
void FN(query_44)(unsigned char *out,const unsigned char *p,const unsigned char *q){ __m128d a,b; __builtin_memcpy(&a,p,16); __builtin_memcpy(&b,q,16); __m128d v=_mm_dp_pd(a,b,255); __builtin_memcpy(out,&v,16); }
void FN(query_45)(unsigned char *out,const unsigned char *p,const unsigned char *q){ __m256 a,b; __builtin_memcpy(&a,p,32); __builtin_memcpy(&b,q,32); __m256 v=_mm256_dp_ps(a,b,0); __builtin_memcpy(out,&v,32); }
void FN(query_46)(unsigned char *out,const unsigned char *p,const unsigned char *q){ __m256 a,b; __builtin_memcpy(&a,p,32); __builtin_memcpy(&b,q,32); __m256 v=_mm256_dp_ps(a,b,1); __builtin_memcpy(out,&v,32); }
void FN(query_47)(unsigned char *out,const unsigned char *p,const unsigned char *q){ __m256 a,b; __builtin_memcpy(&a,p,32); __builtin_memcpy(&b,q,32); __m256 v=_mm256_dp_ps(a,b,18); __builtin_memcpy(out,&v,32); }
void FN(query_48)(unsigned char *out,const unsigned char *p,const unsigned char *q){ __m256 a,b; __builtin_memcpy(&a,p,32); __builtin_memcpy(&b,q,32); __m256 v=_mm256_dp_ps(a,b,127); __builtin_memcpy(out,&v,32); }
void FN(query_49)(unsigned char *out,const unsigned char *p,const unsigned char *q){ __m256 a,b; __builtin_memcpy(&a,p,32); __builtin_memcpy(&b,q,32); __m256 v=_mm256_dp_ps(a,b,241); __builtin_memcpy(out,&v,32); }
void FN(query_50)(unsigned char *out,const unsigned char *p,const unsigned char *q){ __m256 a,b; __builtin_memcpy(&a,p,32); __builtin_memcpy(&b,q,32); __m256 v=_mm256_dp_ps(a,b,255); __builtin_memcpy(out,&v,32); }
