/* Constructors and width casts retain lane-zero memory order. */
#ifndef _KEEL_AVX512_CONSTRUCT_H
#define _KEEL_AVX512_CONSTRUCT_H
typedef signed char __keel_construct_i8x64 __attribute__((vector_size(64)));
typedef short __keel_construct_i16x32 __attribute__((vector_size(64)));

static __inline__ __attribute__((always_inline)) __m512i _mm512_set_epi32(int e15, int e14, int e13, int e12, int e11, int e10, int e9, int e8, int e7, int e6, int e5, int e4, int e3, int e2, int e1, int e0) { return (__m512i)(__keel_avx512_i32x16){e0, e1, e2, e3, e4, e5, e6, e7, e8, e9, e10, e11, e12, e13, e14, e15}; }
static __inline__ __attribute__((always_inline)) __m512i _mm512_setr_epi32(int e0, int e1, int e2, int e3, int e4, int e5, int e6, int e7, int e8, int e9, int e10, int e11, int e12, int e13, int e14, int e15) { return (__m512i)(__keel_avx512_i32x16){e0, e1, e2, e3, e4, e5, e6, e7, e8, e9, e10, e11, e12, e13, e14, e15}; }
static __inline__ __attribute__((always_inline)) __m512i _mm512_set4_epi32(int e3, int e2, int e1, int e0) { return (__m512i)(__keel_avx512_i32x16){e0, e1, e2, e3, e0, e1, e2, e3, e0, e1, e2, e3, e0, e1, e2, e3}; }
static __inline__ __attribute__((always_inline)) __m512i _mm512_setr4_epi32(int e0, int e1, int e2, int e3) { return (__m512i)(__keel_avx512_i32x16){e0, e1, e2, e3, e0, e1, e2, e3, e0, e1, e2, e3, e0, e1, e2, e3}; }

static __inline__ __attribute__((always_inline)) __m512i _mm512_set_epi64(long long e7, long long e6, long long e5, long long e4, long long e3, long long e2, long long e1, long long e0) { return (__m512i)(__m512i){e0, e1, e2, e3, e4, e5, e6, e7}; }
static __inline__ __attribute__((always_inline)) __m512i _mm512_setr_epi64(long long e0, long long e1, long long e2, long long e3, long long e4, long long e5, long long e6, long long e7) { return (__m512i)(__m512i){e0, e1, e2, e3, e4, e5, e6, e7}; }
static __inline__ __attribute__((always_inline)) __m512i _mm512_set4_epi64(long long e3, long long e2, long long e1, long long e0) { return (__m512i)(__m512i){e0, e1, e2, e3, e0, e1, e2, e3}; }
static __inline__ __attribute__((always_inline)) __m512i _mm512_setr4_epi64(long long e0, long long e1, long long e2, long long e3) { return (__m512i)(__m512i){e0, e1, e2, e3, e0, e1, e2, e3}; }

static __inline__ __attribute__((always_inline)) __m512 _mm512_set_ps(float e15, float e14, float e13, float e12, float e11, float e10, float e9, float e8, float e7, float e6, float e5, float e4, float e3, float e2, float e1, float e0) { return (__m512)(__m512){e0, e1, e2, e3, e4, e5, e6, e7, e8, e9, e10, e11, e12, e13, e14, e15}; }
static __inline__ __attribute__((always_inline)) __m512 _mm512_setr_ps(float e0, float e1, float e2, float e3, float e4, float e5, float e6, float e7, float e8, float e9, float e10, float e11, float e12, float e13, float e14, float e15) { return (__m512)(__m512){e0, e1, e2, e3, e4, e5, e6, e7, e8, e9, e10, e11, e12, e13, e14, e15}; }
static __inline__ __attribute__((always_inline)) __m512 _mm512_set4_ps(float e3, float e2, float e1, float e0) { return (__m512)(__m512){e0, e1, e2, e3, e0, e1, e2, e3, e0, e1, e2, e3, e0, e1, e2, e3}; }
static __inline__ __attribute__((always_inline)) __m512 _mm512_setr4_ps(float e0, float e1, float e2, float e3) { return (__m512)(__m512){e0, e1, e2, e3, e0, e1, e2, e3, e0, e1, e2, e3, e0, e1, e2, e3}; }

static __inline__ __attribute__((always_inline)) __m512d _mm512_set_pd(double e7, double e6, double e5, double e4, double e3, double e2, double e1, double e0) { return (__m512d)(__m512d){e0, e1, e2, e3, e4, e5, e6, e7}; }
static __inline__ __attribute__((always_inline)) __m512d _mm512_setr_pd(double e0, double e1, double e2, double e3, double e4, double e5, double e6, double e7) { return (__m512d)(__m512d){e0, e1, e2, e3, e4, e5, e6, e7}; }
static __inline__ __attribute__((always_inline)) __m512d _mm512_set4_pd(double e3, double e2, double e1, double e0) { return (__m512d)(__m512d){e0, e1, e2, e3, e0, e1, e2, e3}; }
static __inline__ __attribute__((always_inline)) __m512d _mm512_setr4_pd(double e0, double e1, double e2, double e3) { return (__m512d)(__m512d){e0, e1, e2, e3, e0, e1, e2, e3}; }

static __inline__ __attribute__((always_inline)) __m512i _mm512_set_epi8(char e63, char e62, char e61, char e60, char e59, char e58, char e57, char e56, char e55, char e54, char e53, char e52, char e51, char e50, char e49, char e48, char e47, char e46, char e45, char e44, char e43, char e42, char e41, char e40, char e39, char e38, char e37, char e36, char e35, char e34, char e33, char e32, char e31, char e30, char e29, char e28, char e27, char e26, char e25, char e24, char e23, char e22, char e21, char e20, char e19, char e18, char e17, char e16, char e15, char e14, char e13, char e12, char e11, char e10, char e9, char e8, char e7, char e6, char e5, char e4, char e3, char e2, char e1, char e0) { return (__m512i)(__keel_construct_i8x64){e0, e1, e2, e3, e4, e5, e6, e7, e8, e9, e10, e11, e12, e13, e14, e15, e16, e17, e18, e19, e20, e21, e22, e23, e24, e25, e26, e27, e28, e29, e30, e31, e32, e33, e34, e35, e36, e37, e38, e39, e40, e41, e42, e43, e44, e45, e46, e47, e48, e49, e50, e51, e52, e53, e54, e55, e56, e57, e58, e59, e60, e61, e62, e63}; }
static __inline__ __attribute__((always_inline)) __m512i _mm512_set1_epi8(char a) { return (__m512i)(__keel_construct_i8x64){a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a}; }
static __inline__ __attribute__((always_inline)) __m512i _mm512_set_epi16(short e31, short e30, short e29, short e28, short e27, short e26, short e25, short e24, short e23, short e22, short e21, short e20, short e19, short e18, short e17, short e16, short e15, short e14, short e13, short e12, short e11, short e10, short e9, short e8, short e7, short e6, short e5, short e4, short e3, short e2, short e1, short e0) { return (__m512i)(__keel_construct_i16x32){e0, e1, e2, e3, e4, e5, e6, e7, e8, e9, e10, e11, e12, e13, e14, e15, e16, e17, e18, e19, e20, e21, e22, e23, e24, e25, e26, e27, e28, e29, e30, e31}; }
static __inline__ __attribute__((always_inline)) __m512i _mm512_set1_epi16(short a) { return (__m512i)(__keel_construct_i16x32){a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a}; }

#define _mm512_setzero _mm512_setzero_ps
#define _mm512_setzero_epi32 _mm512_setzero_si512

/* The standard widening cast leaves upper lanes undefined. Zero is a legal
   choice; zext explicitly guarantees those lanes are zero. */
static __inline__ __attribute__((always_inline)) __m512i _mm512_castsi128_si512(__m128i a) { return (__m512i)_mm512_inserti32x4(((__m512i){0}),(__m128i)a,0); }
static __inline__ __attribute__((always_inline)) __m512i _mm512_zextsi128_si512(__m128i a) { return (__m512i)_mm512_inserti32x4(((__m512i){0}),(__m128i)a,0); }
static __inline__ __attribute__((always_inline)) __m128i _mm512_castsi512_si128(__m512i a) { return (__m128i)_mm512_extracti32x4_epi32((__m512i)a,0); }
static __inline__ __attribute__((always_inline)) __m512i _mm512_castsi256_si512(__m256i a) { return (__m512i)_mm512_inserti64x4(((__m512i){0}),(__m256i)a,0); }
static __inline__ __attribute__((always_inline)) __m512i _mm512_zextsi256_si512(__m256i a) { return (__m512i)_mm512_inserti64x4(((__m512i){0}),(__m256i)a,0); }
static __inline__ __attribute__((always_inline)) __m256i _mm512_castsi512_si256(__m512i a) { return (__m256i)_mm512_extracti64x4_epi64((__m512i)a,0); }
static __inline__ __attribute__((always_inline)) __m512 _mm512_castps128_ps512(__m128 a) { return (__m512)_mm512_inserti32x4(((__m512i){0}),(__m128i)a,0); }
static __inline__ __attribute__((always_inline)) __m512 _mm512_zextps128_ps512(__m128 a) { return (__m512)_mm512_inserti32x4(((__m512i){0}),(__m128i)a,0); }
static __inline__ __attribute__((always_inline)) __m128 _mm512_castps512_ps128(__m512 a) { return (__m128)_mm512_extracti32x4_epi32((__m512i)a,0); }
static __inline__ __attribute__((always_inline)) __m512 _mm512_castps256_ps512(__m256 a) { return (__m512)_mm512_inserti64x4(((__m512i){0}),(__m256i)a,0); }
static __inline__ __attribute__((always_inline)) __m512 _mm512_zextps256_ps512(__m256 a) { return (__m512)_mm512_inserti64x4(((__m512i){0}),(__m256i)a,0); }
static __inline__ __attribute__((always_inline)) __m256 _mm512_castps512_ps256(__m512 a) { return (__m256)_mm512_extracti64x4_epi64((__m512i)a,0); }
static __inline__ __attribute__((always_inline)) __m512d _mm512_castpd128_pd512(__m128d a) { return (__m512d)_mm512_inserti32x4(((__m512i){0}),(__m128i)a,0); }
static __inline__ __attribute__((always_inline)) __m512d _mm512_zextpd128_pd512(__m128d a) { return (__m512d)_mm512_inserti32x4(((__m512i){0}),(__m128i)a,0); }
static __inline__ __attribute__((always_inline)) __m128d _mm512_castpd512_pd128(__m512d a) { return (__m128d)_mm512_extracti32x4_epi32((__m512i)a,0); }
static __inline__ __attribute__((always_inline)) __m512d _mm512_castpd256_pd512(__m256d a) { return (__m512d)_mm512_inserti64x4(((__m512i){0}),(__m256i)a,0); }
static __inline__ __attribute__((always_inline)) __m512d _mm512_zextpd256_pd512(__m256d a) { return (__m512d)_mm512_inserti64x4(((__m512i){0}),(__m256i)a,0); }
static __inline__ __attribute__((always_inline)) __m256d _mm512_castpd512_pd256(__m512d a) { return (__m256d)_mm512_extracti64x4_epi64((__m512i)a,0); }

static __inline__ __attribute__((always_inline)) int _mm512_cvtsi512_si32(__m512i a) { return ((__keel_avx512_i32x16)a)[0]; }
static __inline__ __attribute__((always_inline)) float _mm512_cvtss_f32(__m512 a) { return a[0]; }
static __inline__ __attribute__((always_inline)) double _mm512_cvtsd_f64(__m512d a) { return a[0]; }

#endif
