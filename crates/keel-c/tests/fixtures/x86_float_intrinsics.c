#include <immintrin.h>
#ifdef REFERENCE
#define FN(n) reference_##n
#else
#define FN(n) n
#endif

void FN(fmadd_ps128)(const void *a,const void *b,const void *c,void *out) { __m128 x,y,z,r; __builtin_memcpy(&x,a,16);__builtin_memcpy(&y,b,16);__builtin_memcpy(&z,c,16);r=_mm_fmadd_ps(x,y,z);__builtin_memcpy(out,&r,16); }
void FN(fmadd_pd128)(const void *a,const void *b,const void *c,void *out) { __m128d x,y,z,r; __builtin_memcpy(&x,a,16);__builtin_memcpy(&y,b,16);__builtin_memcpy(&z,c,16);r=_mm_fmadd_pd(x,y,z);__builtin_memcpy(out,&r,16); }
void FN(fmadd_ps256)(const void *a,const void *b,const void *c,void *out) { __m256 x,y,z,r; __builtin_memcpy(&x,a,32);__builtin_memcpy(&y,b,32);__builtin_memcpy(&z,c,32);r=_mm256_fmadd_ps(x,y,z);__builtin_memcpy(out,&r,32); }
void FN(fmadd_pd256)(const void *a,const void *b,const void *c,void *out) { __m256d x,y,z,r; __builtin_memcpy(&x,a,32);__builtin_memcpy(&y,b,32);__builtin_memcpy(&z,c,32);r=_mm256_fmadd_pd(x,y,z);__builtin_memcpy(out,&r,32); }
void FN(fmadd_ss128)(const void *a,const void *b,const void *c,void *out) { __m128 x,y,z,r; __builtin_memcpy(&x,a,16);__builtin_memcpy(&y,b,16);__builtin_memcpy(&z,c,16);r=_mm_fmadd_ss(x,y,z);__builtin_memcpy(out,&r,16); }
void FN(fmadd_sd128)(const void *a,const void *b,const void *c,void *out) { __m128d x,y,z,r; __builtin_memcpy(&x,a,16);__builtin_memcpy(&y,b,16);__builtin_memcpy(&z,c,16);r=_mm_fmadd_sd(x,y,z);__builtin_memcpy(out,&r,16); }
void FN(fmsub_ps128)(const void *a,const void *b,const void *c,void *out) { __m128 x,y,z,r; __builtin_memcpy(&x,a,16);__builtin_memcpy(&y,b,16);__builtin_memcpy(&z,c,16);r=_mm_fmsub_ps(x,y,z);__builtin_memcpy(out,&r,16); }
void FN(fmsub_pd128)(const void *a,const void *b,const void *c,void *out) { __m128d x,y,z,r; __builtin_memcpy(&x,a,16);__builtin_memcpy(&y,b,16);__builtin_memcpy(&z,c,16);r=_mm_fmsub_pd(x,y,z);__builtin_memcpy(out,&r,16); }
void FN(fmsub_ps256)(const void *a,const void *b,const void *c,void *out) { __m256 x,y,z,r; __builtin_memcpy(&x,a,32);__builtin_memcpy(&y,b,32);__builtin_memcpy(&z,c,32);r=_mm256_fmsub_ps(x,y,z);__builtin_memcpy(out,&r,32); }
void FN(fmsub_pd256)(const void *a,const void *b,const void *c,void *out) { __m256d x,y,z,r; __builtin_memcpy(&x,a,32);__builtin_memcpy(&y,b,32);__builtin_memcpy(&z,c,32);r=_mm256_fmsub_pd(x,y,z);__builtin_memcpy(out,&r,32); }
void FN(fmsub_ss128)(const void *a,const void *b,const void *c,void *out) { __m128 x,y,z,r; __builtin_memcpy(&x,a,16);__builtin_memcpy(&y,b,16);__builtin_memcpy(&z,c,16);r=_mm_fmsub_ss(x,y,z);__builtin_memcpy(out,&r,16); }
void FN(fmsub_sd128)(const void *a,const void *b,const void *c,void *out) { __m128d x,y,z,r; __builtin_memcpy(&x,a,16);__builtin_memcpy(&y,b,16);__builtin_memcpy(&z,c,16);r=_mm_fmsub_sd(x,y,z);__builtin_memcpy(out,&r,16); }
void FN(fnmadd_ps128)(const void *a,const void *b,const void *c,void *out) { __m128 x,y,z,r; __builtin_memcpy(&x,a,16);__builtin_memcpy(&y,b,16);__builtin_memcpy(&z,c,16);r=_mm_fnmadd_ps(x,y,z);__builtin_memcpy(out,&r,16); }
void FN(fnmadd_pd128)(const void *a,const void *b,const void *c,void *out) { __m128d x,y,z,r; __builtin_memcpy(&x,a,16);__builtin_memcpy(&y,b,16);__builtin_memcpy(&z,c,16);r=_mm_fnmadd_pd(x,y,z);__builtin_memcpy(out,&r,16); }
void FN(fnmadd_ps256)(const void *a,const void *b,const void *c,void *out) { __m256 x,y,z,r; __builtin_memcpy(&x,a,32);__builtin_memcpy(&y,b,32);__builtin_memcpy(&z,c,32);r=_mm256_fnmadd_ps(x,y,z);__builtin_memcpy(out,&r,32); }
void FN(fnmadd_pd256)(const void *a,const void *b,const void *c,void *out) { __m256d x,y,z,r; __builtin_memcpy(&x,a,32);__builtin_memcpy(&y,b,32);__builtin_memcpy(&z,c,32);r=_mm256_fnmadd_pd(x,y,z);__builtin_memcpy(out,&r,32); }
void FN(fnmadd_ss128)(const void *a,const void *b,const void *c,void *out) { __m128 x,y,z,r; __builtin_memcpy(&x,a,16);__builtin_memcpy(&y,b,16);__builtin_memcpy(&z,c,16);r=_mm_fnmadd_ss(x,y,z);__builtin_memcpy(out,&r,16); }
void FN(fnmadd_sd128)(const void *a,const void *b,const void *c,void *out) { __m128d x,y,z,r; __builtin_memcpy(&x,a,16);__builtin_memcpy(&y,b,16);__builtin_memcpy(&z,c,16);r=_mm_fnmadd_sd(x,y,z);__builtin_memcpy(out,&r,16); }
void FN(fnmsub_ps128)(const void *a,const void *b,const void *c,void *out) { __m128 x,y,z,r; __builtin_memcpy(&x,a,16);__builtin_memcpy(&y,b,16);__builtin_memcpy(&z,c,16);r=_mm_fnmsub_ps(x,y,z);__builtin_memcpy(out,&r,16); }
void FN(fnmsub_pd128)(const void *a,const void *b,const void *c,void *out) { __m128d x,y,z,r; __builtin_memcpy(&x,a,16);__builtin_memcpy(&y,b,16);__builtin_memcpy(&z,c,16);r=_mm_fnmsub_pd(x,y,z);__builtin_memcpy(out,&r,16); }
void FN(fnmsub_ps256)(const void *a,const void *b,const void *c,void *out) { __m256 x,y,z,r; __builtin_memcpy(&x,a,32);__builtin_memcpy(&y,b,32);__builtin_memcpy(&z,c,32);r=_mm256_fnmsub_ps(x,y,z);__builtin_memcpy(out,&r,32); }
void FN(fnmsub_pd256)(const void *a,const void *b,const void *c,void *out) { __m256d x,y,z,r; __builtin_memcpy(&x,a,32);__builtin_memcpy(&y,b,32);__builtin_memcpy(&z,c,32);r=_mm256_fnmsub_pd(x,y,z);__builtin_memcpy(out,&r,32); }
void FN(fnmsub_ss128)(const void *a,const void *b,const void *c,void *out) { __m128 x,y,z,r; __builtin_memcpy(&x,a,16);__builtin_memcpy(&y,b,16);__builtin_memcpy(&z,c,16);r=_mm_fnmsub_ss(x,y,z);__builtin_memcpy(out,&r,16); }
void FN(fnmsub_sd128)(const void *a,const void *b,const void *c,void *out) { __m128d x,y,z,r; __builtin_memcpy(&x,a,16);__builtin_memcpy(&y,b,16);__builtin_memcpy(&z,c,16);r=_mm_fnmsub_sd(x,y,z);__builtin_memcpy(out,&r,16); }
void FN(fmaddsub_ps128)(const void *a,const void *b,const void *c,void *out) { __m128 x,y,z,r; __builtin_memcpy(&x,a,16);__builtin_memcpy(&y,b,16);__builtin_memcpy(&z,c,16);r=_mm_fmaddsub_ps(x,y,z);__builtin_memcpy(out,&r,16); }
void FN(fmaddsub_pd128)(const void *a,const void *b,const void *c,void *out) { __m128d x,y,z,r; __builtin_memcpy(&x,a,16);__builtin_memcpy(&y,b,16);__builtin_memcpy(&z,c,16);r=_mm_fmaddsub_pd(x,y,z);__builtin_memcpy(out,&r,16); }
void FN(fmaddsub_ps256)(const void *a,const void *b,const void *c,void *out) { __m256 x,y,z,r; __builtin_memcpy(&x,a,32);__builtin_memcpy(&y,b,32);__builtin_memcpy(&z,c,32);r=_mm256_fmaddsub_ps(x,y,z);__builtin_memcpy(out,&r,32); }
void FN(fmaddsub_pd256)(const void *a,const void *b,const void *c,void *out) { __m256d x,y,z,r; __builtin_memcpy(&x,a,32);__builtin_memcpy(&y,b,32);__builtin_memcpy(&z,c,32);r=_mm256_fmaddsub_pd(x,y,z);__builtin_memcpy(out,&r,32); }
void FN(fmsubadd_ps128)(const void *a,const void *b,const void *c,void *out) { __m128 x,y,z,r; __builtin_memcpy(&x,a,16);__builtin_memcpy(&y,b,16);__builtin_memcpy(&z,c,16);r=_mm_fmsubadd_ps(x,y,z);__builtin_memcpy(out,&r,16); }
void FN(fmsubadd_pd128)(const void *a,const void *b,const void *c,void *out) { __m128d x,y,z,r; __builtin_memcpy(&x,a,16);__builtin_memcpy(&y,b,16);__builtin_memcpy(&z,c,16);r=_mm_fmsubadd_pd(x,y,z);__builtin_memcpy(out,&r,16); }
void FN(fmsubadd_ps256)(const void *a,const void *b,const void *c,void *out) { __m256 x,y,z,r; __builtin_memcpy(&x,a,32);__builtin_memcpy(&y,b,32);__builtin_memcpy(&z,c,32);r=_mm256_fmsubadd_ps(x,y,z);__builtin_memcpy(out,&r,32); }
void FN(fmsubadd_pd256)(const void *a,const void *b,const void *c,void *out) { __m256d x,y,z,r; __builtin_memcpy(&x,a,32);__builtin_memcpy(&y,b,32);__builtin_memcpy(&z,c,32);r=_mm256_fmsubadd_pd(x,y,z);__builtin_memcpy(out,&r,32); }
void FN(ph128_0)(const void *a,void *out) { __m128 x; __m128i r; __builtin_memcpy(&x,a,16);r=_mm_cvtps_ph(x,0);__builtin_memcpy(out,&r,16); }
void FN(ph256_0)(const void *a,void *out) { __m256 x; __m128i r; __builtin_memcpy(&x,a,32);r=_mm256_cvtps_ph(x,0);__builtin_memcpy(out,&r,16); }
void FN(ph128_1)(const void *a,void *out) { __m128 x; __m128i r; __builtin_memcpy(&x,a,16);r=_mm_cvtps_ph(x,1);__builtin_memcpy(out,&r,16); }
void FN(ph256_1)(const void *a,void *out) { __m256 x; __m128i r; __builtin_memcpy(&x,a,32);r=_mm256_cvtps_ph(x,1);__builtin_memcpy(out,&r,16); }
void FN(ph128_2)(const void *a,void *out) { __m128 x; __m128i r; __builtin_memcpy(&x,a,16);r=_mm_cvtps_ph(x,2);__builtin_memcpy(out,&r,16); }
void FN(ph256_2)(const void *a,void *out) { __m256 x; __m128i r; __builtin_memcpy(&x,a,32);r=_mm256_cvtps_ph(x,2);__builtin_memcpy(out,&r,16); }
void FN(ph128_3)(const void *a,void *out) { __m128 x; __m128i r; __builtin_memcpy(&x,a,16);r=_mm_cvtps_ph(x,3);__builtin_memcpy(out,&r,16); }
void FN(ph256_3)(const void *a,void *out) { __m256 x; __m128i r; __builtin_memcpy(&x,a,32);r=_mm256_cvtps_ph(x,3);__builtin_memcpy(out,&r,16); }
void FN(ph128_4)(const void *a,void *out) { __m128 x; __m128i r; __builtin_memcpy(&x,a,16);r=_mm_cvtps_ph(x,4);__builtin_memcpy(out,&r,16); }
void FN(ph256_4)(const void *a,void *out) { __m256 x; __m128i r; __builtin_memcpy(&x,a,32);r=_mm256_cvtps_ph(x,4);__builtin_memcpy(out,&r,16); }
void FN(ph128_5)(const void *a,void *out) { __m128 x; __m128i r; __builtin_memcpy(&x,a,16);r=_mm_cvtps_ph(x,5);__builtin_memcpy(out,&r,16); }
void FN(ph256_5)(const void *a,void *out) { __m256 x; __m128i r; __builtin_memcpy(&x,a,32);r=_mm256_cvtps_ph(x,5);__builtin_memcpy(out,&r,16); }
void FN(ph128_6)(const void *a,void *out) { __m128 x; __m128i r; __builtin_memcpy(&x,a,16);r=_mm_cvtps_ph(x,6);__builtin_memcpy(out,&r,16); }
void FN(ph256_6)(const void *a,void *out) { __m256 x; __m128i r; __builtin_memcpy(&x,a,32);r=_mm256_cvtps_ph(x,6);__builtin_memcpy(out,&r,16); }
void FN(ph128_7)(const void *a,void *out) { __m128 x; __m128i r; __builtin_memcpy(&x,a,16);r=_mm_cvtps_ph(x,7);__builtin_memcpy(out,&r,16); }
void FN(ph256_7)(const void *a,void *out) { __m256 x; __m128i r; __builtin_memcpy(&x,a,32);r=_mm256_cvtps_ph(x,7);__builtin_memcpy(out,&r,16); }
void FN(ph128_255)(const void *a,void *out) { __m128 x; __m128i r; __builtin_memcpy(&x,a,16);r=_mm_cvtps_ph(x,255);__builtin_memcpy(out,&r,16); }
void FN(ph256_255)(const void *a,void *out) { __m256 x; __m128i r; __builtin_memcpy(&x,a,32);r=_mm256_cvtps_ph(x,255);__builtin_memcpy(out,&r,16); }
void FN(ps128)(const void *a,void *out) { __m128i x; __m128 r;__builtin_memcpy(&x,a,16);r=_mm_cvtph_ps(x);__builtin_memcpy(out,&r,16); }
void FN(ps256)(const void *a,void *out) { __m128i x; __m256 r;__builtin_memcpy(&x,a,16);r=_mm256_cvtph_ps(x);__builtin_memcpy(out,&r,32); }
float FN(scalar_ps)(unsigned short h) { return _cvtsh_ss(h); }
unsigned short FN(scalar_ph)(float f) { return _cvtss_sh(f,255); }
