/* Portable byte/word reductions. Unsigned arithmetic wraps at the lane width.
 * Wide values retain the compiler's ordinary AVX transport requirement. */
#ifndef _KEEL_AVX512_BW_REDUCE_H
#define _KEEL_AVX512_BW_REDUCE_H
static __inline__ __attribute__((always_inline)) unsigned int __keel_bw_reduce_128_8(__m128i a, unsigned int mask, unsigned int op) {
  __keel_i8_128 v=(__keel_i8_128)a;
  unsigned int r=op==1?1:op==2||op==6?255:op==4?127:op==5?128:0;
  for(unsigned int i=0;i<16;i++) if((mask>>i)&1) {
    unsigned int x=(unsigned char)v[i];
    if(op==0) r=(r+x)&255u;
    else if(op==1) r=(r*x)&255u;
    else if(op==2) r&=x;
    else if(op==3) r|=x;
    else if(op==4) { if((x^128u)<(r^128u)) r=x; }
    else if(op==5) { if((x^128u)>(r^128u)) r=x; }
    else if(op==6) { if(x<r) r=x; }
    else { if(x>r) r=x; }
  }
  return r;
}
static __inline__ __attribute__((always_inline)) signed char _mm_reduce_add_epi8(__m128i a) { return (signed char)__keel_bw_reduce_128_8(a,65535u,0); }
static __inline__ __attribute__((always_inline)) signed char _mm_mask_reduce_add_epi8(__mmask16 mask, __m128i a) { return (signed char)__keel_bw_reduce_128_8(a,(unsigned int)mask,0); }
static __inline__ __attribute__((always_inline)) signed char _mm_reduce_mul_epi8(__m128i a) { return (signed char)__keel_bw_reduce_128_8(a,65535u,1); }
static __inline__ __attribute__((always_inline)) signed char _mm_mask_reduce_mul_epi8(__mmask16 mask, __m128i a) { return (signed char)__keel_bw_reduce_128_8(a,(unsigned int)mask,1); }
static __inline__ __attribute__((always_inline)) signed char _mm_reduce_and_epi8(__m128i a) { return (signed char)__keel_bw_reduce_128_8(a,65535u,2); }
static __inline__ __attribute__((always_inline)) signed char _mm_mask_reduce_and_epi8(__mmask16 mask, __m128i a) { return (signed char)__keel_bw_reduce_128_8(a,(unsigned int)mask,2); }
static __inline__ __attribute__((always_inline)) signed char _mm_reduce_or_epi8(__m128i a) { return (signed char)__keel_bw_reduce_128_8(a,65535u,3); }
static __inline__ __attribute__((always_inline)) signed char _mm_mask_reduce_or_epi8(__mmask16 mask, __m128i a) { return (signed char)__keel_bw_reduce_128_8(a,(unsigned int)mask,3); }
static __inline__ __attribute__((always_inline)) signed char _mm_reduce_min_epi8(__m128i a) { return (signed char)__keel_bw_reduce_128_8(a,65535u,4); }
static __inline__ __attribute__((always_inline)) signed char _mm_mask_reduce_min_epi8(__mmask16 mask, __m128i a) { return (signed char)__keel_bw_reduce_128_8(a,(unsigned int)mask,4); }
static __inline__ __attribute__((always_inline)) signed char _mm_reduce_max_epi8(__m128i a) { return (signed char)__keel_bw_reduce_128_8(a,65535u,5); }
static __inline__ __attribute__((always_inline)) signed char _mm_mask_reduce_max_epi8(__mmask16 mask, __m128i a) { return (signed char)__keel_bw_reduce_128_8(a,(unsigned int)mask,5); }
static __inline__ __attribute__((always_inline)) unsigned char _mm_reduce_min_epu8(__m128i a) { return (unsigned char)__keel_bw_reduce_128_8(a,65535u,6); }
static __inline__ __attribute__((always_inline)) unsigned char _mm_mask_reduce_min_epu8(__mmask16 mask, __m128i a) { return (unsigned char)__keel_bw_reduce_128_8(a,(unsigned int)mask,6); }
static __inline__ __attribute__((always_inline)) unsigned char _mm_reduce_max_epu8(__m128i a) { return (unsigned char)__keel_bw_reduce_128_8(a,65535u,7); }
static __inline__ __attribute__((always_inline)) unsigned char _mm_mask_reduce_max_epu8(__mmask16 mask, __m128i a) { return (unsigned char)__keel_bw_reduce_128_8(a,(unsigned int)mask,7); }
static __inline__ __attribute__((always_inline)) unsigned int __keel_bw_reduce_128_16(__m128i a, unsigned int mask, unsigned int op) {
  __keel_i16_128 v=(__keel_i16_128)a;
  unsigned int r=op==1?1:op==2||op==6?65535:op==4?32767:op==5?32768:0;
  for(unsigned int i=0;i<8;i++) if((mask>>i)&1) {
    unsigned int x=(unsigned short)v[i];
    if(op==0) r=(r+x)&65535u;
    else if(op==1) r=(r*x)&65535u;
    else if(op==2) r&=x;
    else if(op==3) r|=x;
    else if(op==4) { if((x^32768u)<(r^32768u)) r=x; }
    else if(op==5) { if((x^32768u)>(r^32768u)) r=x; }
    else if(op==6) { if(x<r) r=x; }
    else { if(x>r) r=x; }
  }
  return r;
}
static __inline__ __attribute__((always_inline)) short _mm_reduce_add_epi16(__m128i a) { return (short)__keel_bw_reduce_128_16(a,255u,0); }
static __inline__ __attribute__((always_inline)) short _mm_mask_reduce_add_epi16(__mmask8 mask, __m128i a) { return (short)__keel_bw_reduce_128_16(a,(unsigned int)mask,0); }
static __inline__ __attribute__((always_inline)) short _mm_reduce_mul_epi16(__m128i a) { return (short)__keel_bw_reduce_128_16(a,255u,1); }
static __inline__ __attribute__((always_inline)) short _mm_mask_reduce_mul_epi16(__mmask8 mask, __m128i a) { return (short)__keel_bw_reduce_128_16(a,(unsigned int)mask,1); }
static __inline__ __attribute__((always_inline)) short _mm_reduce_and_epi16(__m128i a) { return (short)__keel_bw_reduce_128_16(a,255u,2); }
static __inline__ __attribute__((always_inline)) short _mm_mask_reduce_and_epi16(__mmask8 mask, __m128i a) { return (short)__keel_bw_reduce_128_16(a,(unsigned int)mask,2); }
static __inline__ __attribute__((always_inline)) short _mm_reduce_or_epi16(__m128i a) { return (short)__keel_bw_reduce_128_16(a,255u,3); }
static __inline__ __attribute__((always_inline)) short _mm_mask_reduce_or_epi16(__mmask8 mask, __m128i a) { return (short)__keel_bw_reduce_128_16(a,(unsigned int)mask,3); }
static __inline__ __attribute__((always_inline)) short _mm_reduce_min_epi16(__m128i a) { return (short)__keel_bw_reduce_128_16(a,255u,4); }
static __inline__ __attribute__((always_inline)) short _mm_mask_reduce_min_epi16(__mmask16 mask, __m128i a) { return (short)__keel_bw_reduce_128_16(a,(unsigned int)mask,4); }
static __inline__ __attribute__((always_inline)) short _mm_reduce_max_epi16(__m128i a) { return (short)__keel_bw_reduce_128_16(a,255u,5); }
static __inline__ __attribute__((always_inline)) short _mm_mask_reduce_max_epi16(__mmask16 mask, __m128i a) { return (short)__keel_bw_reduce_128_16(a,(unsigned int)mask,5); }
static __inline__ __attribute__((always_inline)) unsigned short _mm_reduce_min_epu16(__m128i a) { return (unsigned short)__keel_bw_reduce_128_16(a,255u,6); }
static __inline__ __attribute__((always_inline)) unsigned short _mm_mask_reduce_min_epu16(__mmask16 mask, __m128i a) { return (unsigned short)__keel_bw_reduce_128_16(a,(unsigned int)mask,6); }
static __inline__ __attribute__((always_inline)) unsigned short _mm_reduce_max_epu16(__m128i a) { return (unsigned short)__keel_bw_reduce_128_16(a,255u,7); }
static __inline__ __attribute__((always_inline)) unsigned short _mm_mask_reduce_max_epu16(__mmask16 mask, __m128i a) { return (unsigned short)__keel_bw_reduce_128_16(a,(unsigned int)mask,7); }
static __inline__ __attribute__((always_inline)) unsigned int __keel_bw_reduce_256_8(__m256i a, unsigned int mask, unsigned int op) {
  __keel_i8_256 v=(__keel_i8_256)a;
  unsigned int r=op==1?1:op==2||op==6?255:op==4?127:op==5?128:0;
  for(unsigned int i=0;i<32;i++) if((mask>>i)&1) {
    unsigned int x=(unsigned char)v[i];
    if(op==0) r=(r+x)&255u;
    else if(op==1) r=(r*x)&255u;
    else if(op==2) r&=x;
    else if(op==3) r|=x;
    else if(op==4) { if((x^128u)<(r^128u)) r=x; }
    else if(op==5) { if((x^128u)>(r^128u)) r=x; }
    else if(op==6) { if(x<r) r=x; }
    else { if(x>r) r=x; }
  }
  return r;
}
static __inline__ __attribute__((always_inline)) signed char _mm256_reduce_add_epi8(__m256i a) { return (signed char)__keel_bw_reduce_256_8(a,4294967295u,0); }
static __inline__ __attribute__((always_inline)) signed char _mm256_mask_reduce_add_epi8(__mmask32 mask, __m256i a) { return (signed char)__keel_bw_reduce_256_8(a,(unsigned int)mask,0); }
static __inline__ __attribute__((always_inline)) signed char _mm256_reduce_mul_epi8(__m256i a) { return (signed char)__keel_bw_reduce_256_8(a,4294967295u,1); }
static __inline__ __attribute__((always_inline)) signed char _mm256_mask_reduce_mul_epi8(__mmask32 mask, __m256i a) { return (signed char)__keel_bw_reduce_256_8(a,(unsigned int)mask,1); }
static __inline__ __attribute__((always_inline)) signed char _mm256_reduce_and_epi8(__m256i a) { return (signed char)__keel_bw_reduce_256_8(a,4294967295u,2); }
static __inline__ __attribute__((always_inline)) signed char _mm256_mask_reduce_and_epi8(__mmask32 mask, __m256i a) { return (signed char)__keel_bw_reduce_256_8(a,(unsigned int)mask,2); }
static __inline__ __attribute__((always_inline)) signed char _mm256_reduce_or_epi8(__m256i a) { return (signed char)__keel_bw_reduce_256_8(a,4294967295u,3); }
static __inline__ __attribute__((always_inline)) signed char _mm256_mask_reduce_or_epi8(__mmask32 mask, __m256i a) { return (signed char)__keel_bw_reduce_256_8(a,(unsigned int)mask,3); }
static __inline__ __attribute__((always_inline)) signed char _mm256_reduce_min_epi8(__m256i a) { return (signed char)__keel_bw_reduce_256_8(a,4294967295u,4); }
static __inline__ __attribute__((always_inline)) signed char _mm256_mask_reduce_min_epi8(__mmask32 mask, __m256i a) { return (signed char)__keel_bw_reduce_256_8(a,(unsigned int)mask,4); }
static __inline__ __attribute__((always_inline)) signed char _mm256_reduce_max_epi8(__m256i a) { return (signed char)__keel_bw_reduce_256_8(a,4294967295u,5); }
static __inline__ __attribute__((always_inline)) signed char _mm256_mask_reduce_max_epi8(__mmask32 mask, __m256i a) { return (signed char)__keel_bw_reduce_256_8(a,(unsigned int)mask,5); }
static __inline__ __attribute__((always_inline)) unsigned char _mm256_reduce_min_epu8(__m256i a) { return (unsigned char)__keel_bw_reduce_256_8(a,4294967295u,6); }
static __inline__ __attribute__((always_inline)) unsigned char _mm256_mask_reduce_min_epu8(__mmask32 mask, __m256i a) { return (unsigned char)__keel_bw_reduce_256_8(a,(unsigned int)mask,6); }
static __inline__ __attribute__((always_inline)) unsigned char _mm256_reduce_max_epu8(__m256i a) { return (unsigned char)__keel_bw_reduce_256_8(a,4294967295u,7); }
static __inline__ __attribute__((always_inline)) unsigned char _mm256_mask_reduce_max_epu8(__mmask32 mask, __m256i a) { return (unsigned char)__keel_bw_reduce_256_8(a,(unsigned int)mask,7); }
static __inline__ __attribute__((always_inline)) unsigned int __keel_bw_reduce_256_16(__m256i a, unsigned int mask, unsigned int op) {
  __keel_i16_256 v=(__keel_i16_256)a;
  unsigned int r=op==1?1:op==2||op==6?65535:op==4?32767:op==5?32768:0;
  for(unsigned int i=0;i<16;i++) if((mask>>i)&1) {
    unsigned int x=(unsigned short)v[i];
    if(op==0) r=(r+x)&65535u;
    else if(op==1) r=(r*x)&65535u;
    else if(op==2) r&=x;
    else if(op==3) r|=x;
    else if(op==4) { if((x^32768u)<(r^32768u)) r=x; }
    else if(op==5) { if((x^32768u)>(r^32768u)) r=x; }
    else if(op==6) { if(x<r) r=x; }
    else { if(x>r) r=x; }
  }
  return r;
}
static __inline__ __attribute__((always_inline)) short _mm256_reduce_add_epi16(__m256i a) { return (short)__keel_bw_reduce_256_16(a,65535u,0); }
static __inline__ __attribute__((always_inline)) short _mm256_mask_reduce_add_epi16(__mmask16 mask, __m256i a) { return (short)__keel_bw_reduce_256_16(a,(unsigned int)mask,0); }
static __inline__ __attribute__((always_inline)) short _mm256_reduce_mul_epi16(__m256i a) { return (short)__keel_bw_reduce_256_16(a,65535u,1); }
static __inline__ __attribute__((always_inline)) short _mm256_mask_reduce_mul_epi16(__mmask16 mask, __m256i a) { return (short)__keel_bw_reduce_256_16(a,(unsigned int)mask,1); }
static __inline__ __attribute__((always_inline)) short _mm256_reduce_and_epi16(__m256i a) { return (short)__keel_bw_reduce_256_16(a,65535u,2); }
static __inline__ __attribute__((always_inline)) short _mm256_mask_reduce_and_epi16(__mmask16 mask, __m256i a) { return (short)__keel_bw_reduce_256_16(a,(unsigned int)mask,2); }
static __inline__ __attribute__((always_inline)) short _mm256_reduce_or_epi16(__m256i a) { return (short)__keel_bw_reduce_256_16(a,65535u,3); }
static __inline__ __attribute__((always_inline)) short _mm256_mask_reduce_or_epi16(__mmask16 mask, __m256i a) { return (short)__keel_bw_reduce_256_16(a,(unsigned int)mask,3); }
static __inline__ __attribute__((always_inline)) short _mm256_reduce_min_epi16(__m256i a) { return (short)__keel_bw_reduce_256_16(a,65535u,4); }
static __inline__ __attribute__((always_inline)) short _mm256_mask_reduce_min_epi16(__mmask16 mask, __m256i a) { return (short)__keel_bw_reduce_256_16(a,(unsigned int)mask,4); }
static __inline__ __attribute__((always_inline)) short _mm256_reduce_max_epi16(__m256i a) { return (short)__keel_bw_reduce_256_16(a,65535u,5); }
static __inline__ __attribute__((always_inline)) short _mm256_mask_reduce_max_epi16(__mmask16 mask, __m256i a) { return (short)__keel_bw_reduce_256_16(a,(unsigned int)mask,5); }
static __inline__ __attribute__((always_inline)) unsigned short _mm256_reduce_min_epu16(__m256i a) { return (unsigned short)__keel_bw_reduce_256_16(a,65535u,6); }
static __inline__ __attribute__((always_inline)) unsigned short _mm256_mask_reduce_min_epu16(__mmask16 mask, __m256i a) { return (unsigned short)__keel_bw_reduce_256_16(a,(unsigned int)mask,6); }
static __inline__ __attribute__((always_inline)) unsigned short _mm256_reduce_max_epu16(__m256i a) { return (unsigned short)__keel_bw_reduce_256_16(a,65535u,7); }
static __inline__ __attribute__((always_inline)) unsigned short _mm256_mask_reduce_max_epu16(__mmask16 mask, __m256i a) { return (unsigned short)__keel_bw_reduce_256_16(a,(unsigned int)mask,7); }
#endif
