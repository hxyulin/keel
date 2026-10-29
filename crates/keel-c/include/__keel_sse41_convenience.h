#ifndef _KEEL_SSE41_CONVENIENCE_H
#define _KEEL_SSE41_CONVENIENCE_H
static __inline__ __attribute__((always_inline)) int _mm_extract_epi8(__m128i a,int i) { return (int)((__keel_u8_128)a)[i & 15]; }
static __inline__ __attribute__((always_inline)) __m128i _mm_insert_epi8(__m128i a,int x,int i) { __keel_u8_128 r=(__keel_u8_128)a;r[i & 15]=x;return (__m128i)r; }
static __inline__ __attribute__((always_inline)) long long _mm_extract_epi64(__m128i a,int i) { return (long long)((__keel_i64_128)a)[i & 1]; }
static __inline__ __attribute__((always_inline)) __m128i _mm_insert_epi64(__m128i a,long long x,int i) { __keel_i64_128 r=(__keel_i64_128)a;r[i & 1]=x;return (__m128i)r; }
static __inline__ __attribute__((always_inline)) int _mm_extract_ps(__m128 a,int i) { return ((__keel_i32_128)a)[i & 3]; }
#define _mm_insert_ps(a,b,i) __extension__ ({ _Static_assert(((i)|0)>=0 && ((i)|0)<=255,"SIMD lane control must be an integer constant expression in 0..255"); __m128 __keel_a=(a),__keel_b=(b); __m128 __keel_r=__builtin_shufflevector(__keel_a,__keel_b,((((i)>>4)&3)==0 ? 4+(((i)>>6)&3) : 0),((((i)>>4)&3)==1 ? 4+(((i)>>6)&3) : 1),((((i)>>4)&3)==2 ? 4+(((i)>>6)&3) : 2),((((i)>>4)&3)==3 ? 4+(((i)>>6)&3) : 3)); __keel_i32_128 __keel_mask={(((i)&1)?0:-1),(((i)&2)?0:-1),(((i)&4)?0:-1),(((i)&8)?0:-1)}; (__m128)((__keel_i32_128)__keel_r & __keel_mask); })
#endif /* _KEEL_SSE41_CONVENIENCE_H */
