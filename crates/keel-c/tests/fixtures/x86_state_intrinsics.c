#include <immintrin.h>
unsigned state_csr(unsigned value) {
  unsigned before=_mm_getcsr();
  _mm_setcsr(value);
  unsigned during=_mm_getcsr();
  _mm_setcsr(before);
  return during;
}
unsigned state_fields(unsigned value,unsigned *count) {
  unsigned before=_mm_getcsr();
  _MM_SET_ROUNDING_MODE((++*count,value));
  unsigned during=_mm_getcsr();
  _mm_setcsr(before);
  return during;
}
void state_zero(const void *input,void *out) {
  __m256 a;__m256d b;__m256i c;
  __builtin_memcpy(&a,input,32);
  __builtin_memcpy(&b,(const char*)input+32,32);
  __builtin_memcpy(&c,(const char*)input+64,32);
  _mm256_zeroupper();
  _mm256_zeroall();
  __builtin_memcpy(out,&a,32);
  __builtin_memcpy((char*)out+32,&b,32);
  __builtin_memcpy((char*)out+64,&c,32);
}
void state_stream(const void*input,void*out,void*loaded) {
  __m128 a;__m128d b;__m128i c;__m256 d;__m256d e;__m256i f;int x;long long y;
  __builtin_memcpy(&a,input,16);__builtin_memcpy(&b,input,16);__builtin_memcpy(&c,input,16);
  __builtin_memcpy(&d,input,32);__builtin_memcpy(&e,input,32);__builtin_memcpy(&f,input,32);
  __builtin_memcpy(&x,input,4);__builtin_memcpy(&y,input,8);
  _mm_stream_ps((float*)out,a);
  _mm_stream_pd((double*)((char*)out+16),b);
  _mm_stream_si128((char*)out+32,c);
  _mm256_stream_ps((float*)((char*)out+64),d);
  _mm256_stream_pd((double*)((char*)out+96),e);
  _mm256_stream_si256((char*)out+128,f);
  _mm_stream_si32((char*)out+160,x);
  _mm_stream_si64((char*)out+168,y);
  _mm_sfence();
  c=_mm_stream_load_si128((char*)out+32);
  f=_mm256_stream_load_si256((char*)out+128);
  _mm_lfence();_mm_mfence();_mm_pause();
  __builtin_memcpy(loaded,&c,16);__builtin_memcpy((char*)loaded+16,&f,32);
}
void state_prefetch(const void*address,unsigned*count) {
  _mm_prefetch((++*count,address),_MM_HINT_NTA);
  _mm_prefetch((++*count,address),_MM_HINT_T0);
  _mm_prefetch((++*count,address),_MM_HINT_T1);
  _mm_prefetch((++*count,address),_MM_HINT_T2);
  _mm_prefetch((++*count,address),_MM_HINT_ET0);
  _mm_prefetch((++*count,address),_MM_HINT_ET1);
}
void state_maskmove(const void*input,const void*mask,void*out) {
  __m128i data,bits;__builtin_memcpy(&data,input,16);__builtin_memcpy(&bits,mask,16);
  _mm_maskmoveu_si128(data,bits,(char*)out);
}
