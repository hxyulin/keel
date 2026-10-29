#ifndef _KEEL_AVX_CONVENIENCE_H
#define _KEEL_AVX_CONVENIENCE_H
static __inline__ __attribute__((always_inline)) __m256 _mm256_undefined_ps(void) { return (__m256){0}; }
static __inline__ __attribute__((always_inline)) __m256d _mm256_undefined_pd(void) { return (__m256d){0}; }
static __inline__ __attribute__((always_inline)) __m256i _mm256_undefined_si256(void) { return (__m256i){0}; }
static __inline__ __attribute__((always_inline)) __m256i _mm256_lddqu_si256(const __m256i_u *p) { __m256i r;__builtin_memcpy(&r,p,32);return r; }
static __inline__ __attribute__((always_inline)) float _mm256_cvtss_f32(__m256 a) { return a[0]; }
static __inline__ __attribute__((always_inline)) double _mm256_cvtsd_f64(__m256d a) { return a[0]; }
static __inline__ __attribute__((always_inline)) int _mm256_cvtsi256_si32(__m256i a) { return ((__keel_i32_256)a)[0]; }
static __inline__ __attribute__((always_inline)) int _mm256_extract_epi8(__m256i a,int i) { return (int)((__keel_u8_256)a)[i & 31]; }
static __inline__ __attribute__((always_inline)) __m256i _mm256_insert_epi8(__m256i a,int x,int i) { __keel_u8_256 r=(__keel_u8_256)a;r[i & 31]=x;return (__m256i)r; }
static __inline__ __attribute__((always_inline)) int _mm256_extract_epi16(__m256i a,int i) { return (int)((__keel_u16_256)a)[i & 15]; }
static __inline__ __attribute__((always_inline)) __m256i _mm256_insert_epi16(__m256i a,int x,int i) { __keel_u16_256 r=(__keel_u16_256)a;r[i & 15]=x;return (__m256i)r; }
static __inline__ __attribute__((always_inline)) int _mm256_extract_epi32(__m256i a,int i) { return (int)((__keel_i32_256)a)[i & 7]; }
static __inline__ __attribute__((always_inline)) __m256i _mm256_insert_epi32(__m256i a,int x,int i) { __keel_i32_256 r=(__keel_i32_256)a;r[i & 7]=x;return (__m256i)r; }
static __inline__ __attribute__((always_inline)) long long _mm256_extract_epi64(__m256i a,int i) { return (long long)((__keel_i64_256)a)[i & 3]; }
static __inline__ __attribute__((always_inline)) __m256i _mm256_insert_epi64(__m256i a,long long x,int i) { __keel_i64_256 r=(__keel_i64_256)a;r[i & 3]=x;return (__m256i)r; }
static __inline__ __attribute__((always_inline)) __m256 _mm256_loadu2_m128(const float*hi,const float*lo) { __m128 a,b;__builtin_memcpy(&a,lo,16);__builtin_memcpy(&b,hi,16);return (__m256){a[0],a[1],a[2],a[3],b[0],b[1],b[2],b[3]}; }
static __inline__ __attribute__((always_inline)) void _mm256_storeu2_m128(float*hi,float*lo,__m256 x) { __m128 a={x[0],x[1],x[2],x[3]},b={x[4],x[5],x[6],x[7]};__builtin_memcpy(lo,&a,16);__builtin_memcpy(hi,&b,16); }
static __inline__ __attribute__((always_inline)) __m256d _mm256_loadu2_m128d(const double*hi,const double*lo) { __m128d a,b;__builtin_memcpy(&a,lo,16);__builtin_memcpy(&b,hi,16);return (__m256d){a[0],a[1],b[0],b[1]}; }
static __inline__ __attribute__((always_inline)) void _mm256_storeu2_m128d(double*hi,double*lo,__m256d x) { __m128d a={x[0],x[1]},b={x[2],x[3]};__builtin_memcpy(lo,&a,16);__builtin_memcpy(hi,&b,16); }
static __inline__ __attribute__((always_inline)) __m256i _mm256_loadu2_m128i(const __m128i_u*hi,const __m128i_u*lo) { __m128i a,b;__builtin_memcpy(&a,lo,16);__builtin_memcpy(&b,hi,16);return (__m256i){a[0],a[1],b[0],b[1]}; }
static __inline__ __attribute__((always_inline)) void _mm256_storeu2_m128i(__m128i_u*hi,__m128i_u*lo,__m256i x) { __m128i a={x[0],x[1]},b={x[2],x[3]};__builtin_memcpy(lo,&a,16);__builtin_memcpy(hi,&b,16); }
#endif /* _KEEL_AVX_CONVENIENCE_H */
