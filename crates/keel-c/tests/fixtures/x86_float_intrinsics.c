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

void FN(lane_hadd_ps128)(const void*a,const void*b,const void*c,void*out){(void)c;__m128 x,y,r;__builtin_memcpy(&x,a,16);__builtin_memcpy(&y,b,16);r=_mm_hadd_ps(x,y);__builtin_memcpy(out,&r,16);}
#ifndef REFERENCE
void dead_lane_hadd_ps128(const void*a,const void*b,const void*c,void*out){(void)c;(void)out;__m128 x,y;__builtin_memcpy(&x,a,16);__builtin_memcpy(&y,b,16);(void)_mm_hadd_ps(x,y);}
#endif
void FN(lane_hsub_ps128)(const void*a,const void*b,const void*c,void*out){(void)c;__m128 x,y,r;__builtin_memcpy(&x,a,16);__builtin_memcpy(&y,b,16);r=_mm_hsub_ps(x,y);__builtin_memcpy(out,&r,16);}
#ifndef REFERENCE
void dead_lane_hsub_ps128(const void*a,const void*b,const void*c,void*out){(void)c;(void)out;__m128 x,y;__builtin_memcpy(&x,a,16);__builtin_memcpy(&y,b,16);(void)_mm_hsub_ps(x,y);}
#endif
void FN(lane_hadd_pd128)(const void*a,const void*b,const void*c,void*out){(void)c;__m128d x,y,r;__builtin_memcpy(&x,a,16);__builtin_memcpy(&y,b,16);r=_mm_hadd_pd(x,y);__builtin_memcpy(out,&r,16);}
#ifndef REFERENCE
void dead_lane_hadd_pd128(const void*a,const void*b,const void*c,void*out){(void)c;(void)out;__m128d x,y;__builtin_memcpy(&x,a,16);__builtin_memcpy(&y,b,16);(void)_mm_hadd_pd(x,y);}
#endif
void FN(lane_hsub_pd128)(const void*a,const void*b,const void*c,void*out){(void)c;__m128d x,y,r;__builtin_memcpy(&x,a,16);__builtin_memcpy(&y,b,16);r=_mm_hsub_pd(x,y);__builtin_memcpy(out,&r,16);}
#ifndef REFERENCE
void dead_lane_hsub_pd128(const void*a,const void*b,const void*c,void*out){(void)c;(void)out;__m128d x,y;__builtin_memcpy(&x,a,16);__builtin_memcpy(&y,b,16);(void)_mm_hsub_pd(x,y);}
#endif
void FN(lane_unpacklo_ps128)(const void*a,const void*b,const void*c,void*out){(void)c;__m128 x,y,r;__builtin_memcpy(&x,a,16);__builtin_memcpy(&y,b,16);r=_mm_unpacklo_ps(x,y);__builtin_memcpy(out,&r,16);}
void FN(lane_unpackhi_ps128)(const void*a,const void*b,const void*c,void*out){(void)c;__m128 x,y,r;__builtin_memcpy(&x,a,16);__builtin_memcpy(&y,b,16);r=_mm_unpackhi_ps(x,y);__builtin_memcpy(out,&r,16);}
void FN(lane_unpacklo_pd128)(const void*a,const void*b,const void*c,void*out){(void)c;__m128d x,y,r;__builtin_memcpy(&x,a,16);__builtin_memcpy(&y,b,16);r=_mm_unpacklo_pd(x,y);__builtin_memcpy(out,&r,16);}
void FN(lane_unpackhi_pd128)(const void*a,const void*b,const void*c,void*out){(void)c;__m128d x,y,r;__builtin_memcpy(&x,a,16);__builtin_memcpy(&y,b,16);r=_mm_unpackhi_pd(x,y);__builtin_memcpy(out,&r,16);}
void FN(lane_hadd_ps256)(const void*a,const void*b,const void*c,void*out){(void)c;__m256 x,y,r;__builtin_memcpy(&x,a,32);__builtin_memcpy(&y,b,32);r=_mm256_hadd_ps(x,y);__builtin_memcpy(out,&r,32);}
#ifndef REFERENCE
void dead_lane_hadd_ps256(const void*a,const void*b,const void*c,void*out){(void)c;(void)out;__m256 x,y;__builtin_memcpy(&x,a,32);__builtin_memcpy(&y,b,32);(void)_mm256_hadd_ps(x,y);}
#endif
void FN(lane_hsub_ps256)(const void*a,const void*b,const void*c,void*out){(void)c;__m256 x,y,r;__builtin_memcpy(&x,a,32);__builtin_memcpy(&y,b,32);r=_mm256_hsub_ps(x,y);__builtin_memcpy(out,&r,32);}
#ifndef REFERENCE
void dead_lane_hsub_ps256(const void*a,const void*b,const void*c,void*out){(void)c;(void)out;__m256 x,y;__builtin_memcpy(&x,a,32);__builtin_memcpy(&y,b,32);(void)_mm256_hsub_ps(x,y);}
#endif
void FN(lane_hadd_pd256)(const void*a,const void*b,const void*c,void*out){(void)c;__m256d x,y,r;__builtin_memcpy(&x,a,32);__builtin_memcpy(&y,b,32);r=_mm256_hadd_pd(x,y);__builtin_memcpy(out,&r,32);}
#ifndef REFERENCE
void dead_lane_hadd_pd256(const void*a,const void*b,const void*c,void*out){(void)c;(void)out;__m256d x,y;__builtin_memcpy(&x,a,32);__builtin_memcpy(&y,b,32);(void)_mm256_hadd_pd(x,y);}
#endif
void FN(lane_hsub_pd256)(const void*a,const void*b,const void*c,void*out){(void)c;__m256d x,y,r;__builtin_memcpy(&x,a,32);__builtin_memcpy(&y,b,32);r=_mm256_hsub_pd(x,y);__builtin_memcpy(out,&r,32);}
#ifndef REFERENCE
void dead_lane_hsub_pd256(const void*a,const void*b,const void*c,void*out){(void)c;(void)out;__m256d x,y;__builtin_memcpy(&x,a,32);__builtin_memcpy(&y,b,32);(void)_mm256_hsub_pd(x,y);}
#endif
void FN(lane_unpacklo_ps256)(const void*a,const void*b,const void*c,void*out){(void)c;__m256 x,y,r;__builtin_memcpy(&x,a,32);__builtin_memcpy(&y,b,32);r=_mm256_unpacklo_ps(x,y);__builtin_memcpy(out,&r,32);}
void FN(lane_unpackhi_ps256)(const void*a,const void*b,const void*c,void*out){(void)c;__m256 x,y,r;__builtin_memcpy(&x,a,32);__builtin_memcpy(&y,b,32);r=_mm256_unpackhi_ps(x,y);__builtin_memcpy(out,&r,32);}
void FN(lane_unpacklo_pd256)(const void*a,const void*b,const void*c,void*out){(void)c;__m256d x,y,r;__builtin_memcpy(&x,a,32);__builtin_memcpy(&y,b,32);r=_mm256_unpacklo_pd(x,y);__builtin_memcpy(out,&r,32);}
void FN(lane_unpackhi_pd256)(const void*a,const void*b,const void*c,void*out){(void)c;__m256d x,y,r;__builtin_memcpy(&x,a,32);__builtin_memcpy(&y,b,32);r=_mm256_unpackhi_pd(x,y);__builtin_memcpy(out,&r,32);}

void FN(lane_sqrt_ps128)(const void*a,const void*b,const void*c,void*out){(void)b;(void)c;__m128 x;__builtin_memcpy(&x,a,16);__m128 r=_mm_sqrt_ps(x);__builtin_memcpy(out,&r,16);}
#ifndef REFERENCE
void dead_lane_sqrt_ps128(const void*a,const void*b,const void*c,void*out){(void)b;(void)c;(void)out;__m128 x;__builtin_memcpy(&x,a,16);(void)_mm_sqrt_ps(x);}
#endif
void FN(lane_addsub_ps128)(const void*a,const void*b,const void*c,void*out){(void)b;(void)c;__m128 x;__builtin_memcpy(&x,a,16);__m128 y;__builtin_memcpy(&y,b,16);__m128 r=_mm_addsub_ps(x,y);__builtin_memcpy(out,&r,16);}
#ifndef REFERENCE
void dead_lane_addsub_ps128(const void*a,const void*b,const void*c,void*out){(void)b;(void)c;(void)out;__m128 x;__builtin_memcpy(&x,a,16);__m128 y;__builtin_memcpy(&y,b,16);(void)_mm_addsub_ps(x,y);}
#endif
void FN(lane_sqrt_pd128)(const void*a,const void*b,const void*c,void*out){(void)b;(void)c;__m128d x;__builtin_memcpy(&x,a,16);__m128d r=_mm_sqrt_pd(x);__builtin_memcpy(out,&r,16);}
#ifndef REFERENCE
void dead_lane_sqrt_pd128(const void*a,const void*b,const void*c,void*out){(void)b;(void)c;(void)out;__m128d x;__builtin_memcpy(&x,a,16);(void)_mm_sqrt_pd(x);}
#endif
void FN(lane_addsub_pd128)(const void*a,const void*b,const void*c,void*out){(void)b;(void)c;__m128d x;__builtin_memcpy(&x,a,16);__m128d y;__builtin_memcpy(&y,b,16);__m128d r=_mm_addsub_pd(x,y);__builtin_memcpy(out,&r,16);}
#ifndef REFERENCE
void dead_lane_addsub_pd128(const void*a,const void*b,const void*c,void*out){(void)b;(void)c;(void)out;__m128d x;__builtin_memcpy(&x,a,16);__m128d y;__builtin_memcpy(&y,b,16);(void)_mm_addsub_pd(x,y);}
#endif
void FN(lane_sqrt_ps256)(const void*a,const void*b,const void*c,void*out){(void)b;(void)c;__m256 x;__builtin_memcpy(&x,a,32);__m256 r=_mm256_sqrt_ps(x);__builtin_memcpy(out,&r,32);}
#ifndef REFERENCE
void dead_lane_sqrt_ps256(const void*a,const void*b,const void*c,void*out){(void)b;(void)c;(void)out;__m256 x;__builtin_memcpy(&x,a,32);(void)_mm256_sqrt_ps(x);}
#endif
void FN(lane_addsub_ps256)(const void*a,const void*b,const void*c,void*out){(void)b;(void)c;__m256 x;__builtin_memcpy(&x,a,32);__m256 y;__builtin_memcpy(&y,b,32);__m256 r=_mm256_addsub_ps(x,y);__builtin_memcpy(out,&r,32);}
#ifndef REFERENCE
void dead_lane_addsub_ps256(const void*a,const void*b,const void*c,void*out){(void)b;(void)c;(void)out;__m256 x;__builtin_memcpy(&x,a,32);__m256 y;__builtin_memcpy(&y,b,32);(void)_mm256_addsub_ps(x,y);}
#endif
void FN(lane_sqrt_pd256)(const void*a,const void*b,const void*c,void*out){(void)b;(void)c;__m256d x;__builtin_memcpy(&x,a,32);__m256d r=_mm256_sqrt_pd(x);__builtin_memcpy(out,&r,32);}
#ifndef REFERENCE
void dead_lane_sqrt_pd256(const void*a,const void*b,const void*c,void*out){(void)b;(void)c;(void)out;__m256d x;__builtin_memcpy(&x,a,32);(void)_mm256_sqrt_pd(x);}
#endif
void FN(lane_addsub_pd256)(const void*a,const void*b,const void*c,void*out){(void)b;(void)c;__m256d x;__builtin_memcpy(&x,a,32);__m256d y;__builtin_memcpy(&y,b,32);__m256d r=_mm256_addsub_pd(x,y);__builtin_memcpy(out,&r,32);}
#ifndef REFERENCE
void dead_lane_addsub_pd256(const void*a,const void*b,const void*c,void*out){(void)b;(void)c;(void)out;__m256d x;__builtin_memcpy(&x,a,32);__m256d y;__builtin_memcpy(&y,b,32);(void)_mm256_addsub_pd(x,y);}
#endif
