#include <immintrin.h>
static int mark(unsigned *count) { ++*count; return 0; }
void construct_0(unsigned char *out,const unsigned char *input,unsigned *count) { float x0; __builtin_memcpy(&x0,input+0,4);float x1; __builtin_memcpy(&x1,input+4,4);float x2; __builtin_memcpy(&x2,input+8,4);float x3; __builtin_memcpy(&x3,input+12,4);__m128 result=_mm_setr_ps((mark(count),x0),(mark(count),x1),(mark(count),x2),(mark(count),x3)); __builtin_memcpy(out,&result,16); }
void construct_1(unsigned char *out,const unsigned char *input,unsigned *count) { double x0; __builtin_memcpy(&x0,input+0,8);double x1; __builtin_memcpy(&x1,input+8,8);__m128d result=_mm_setr_pd((mark(count),x0),(mark(count),x1)); __builtin_memcpy(out,&result,16); }
void construct_2(unsigned char *out,const unsigned char *input,unsigned *count) { float x0; __builtin_memcpy(&x0,input+0,4);float x1; __builtin_memcpy(&x1,input+4,4);float x2; __builtin_memcpy(&x2,input+8,4);float x3; __builtin_memcpy(&x3,input+12,4);float x4; __builtin_memcpy(&x4,input+16,4);float x5; __builtin_memcpy(&x5,input+20,4);float x6; __builtin_memcpy(&x6,input+24,4);float x7; __builtin_memcpy(&x7,input+28,4);__m256 result=_mm256_setr_ps((mark(count),x0),(mark(count),x1),(mark(count),x2),(mark(count),x3),(mark(count),x4),(mark(count),x5),(mark(count),x6),(mark(count),x7)); __builtin_memcpy(out,&result,32); }
void construct_3(unsigned char *out,const unsigned char *input,unsigned *count) { double x0; __builtin_memcpy(&x0,input+0,8);double x1; __builtin_memcpy(&x1,input+8,8);double x2; __builtin_memcpy(&x2,input+16,8);double x3; __builtin_memcpy(&x3,input+24,8);__m256d result=_mm256_setr_pd((mark(count),x0),(mark(count),x1),(mark(count),x2),(mark(count),x3)); __builtin_memcpy(out,&result,32); }
void construct_4(unsigned char *out,const unsigned char *input,unsigned *count) { char x0; __builtin_memcpy(&x0,input+0,1);char x1; __builtin_memcpy(&x1,input+1,1);char x2; __builtin_memcpy(&x2,input+2,1);char x3; __builtin_memcpy(&x3,input+3,1);char x4; __builtin_memcpy(&x4,input+4,1);char x5; __builtin_memcpy(&x5,input+5,1);char x6; __builtin_memcpy(&x6,input+6,1);char x7; __builtin_memcpy(&x7,input+7,1);char x8; __builtin_memcpy(&x8,input+8,1);char x9; __builtin_memcpy(&x9,input+9,1);char x10; __builtin_memcpy(&x10,input+10,1);char x11; __builtin_memcpy(&x11,input+11,1);char x12; __builtin_memcpy(&x12,input+12,1);char x13; __builtin_memcpy(&x13,input+13,1);char x14; __builtin_memcpy(&x14,input+14,1);char x15; __builtin_memcpy(&x15,input+15,1);__m128i result=_mm_setr_epi8((mark(count),x0),(mark(count),x1),(mark(count),x2),(mark(count),x3),(mark(count),x4),(mark(count),x5),(mark(count),x6),(mark(count),x7),(mark(count),x8),(mark(count),x9),(mark(count),x10),(mark(count),x11),(mark(count),x12),(mark(count),x13),(mark(count),x14),(mark(count),x15)); __builtin_memcpy(out,&result,16); }
void construct_5(unsigned char *out,const unsigned char *input,unsigned *count) { char x15; __builtin_memcpy(&x15,input+0,1);char x14; __builtin_memcpy(&x14,input+1,1);char x13; __builtin_memcpy(&x13,input+2,1);char x12; __builtin_memcpy(&x12,input+3,1);char x11; __builtin_memcpy(&x11,input+4,1);char x10; __builtin_memcpy(&x10,input+5,1);char x9; __builtin_memcpy(&x9,input+6,1);char x8; __builtin_memcpy(&x8,input+7,1);char x7; __builtin_memcpy(&x7,input+8,1);char x6; __builtin_memcpy(&x6,input+9,1);char x5; __builtin_memcpy(&x5,input+10,1);char x4; __builtin_memcpy(&x4,input+11,1);char x3; __builtin_memcpy(&x3,input+12,1);char x2; __builtin_memcpy(&x2,input+13,1);char x1; __builtin_memcpy(&x1,input+14,1);char x0; __builtin_memcpy(&x0,input+15,1);__m128i result=_mm_set_epi8((mark(count),x15),(mark(count),x14),(mark(count),x13),(mark(count),x12),(mark(count),x11),(mark(count),x10),(mark(count),x9),(mark(count),x8),(mark(count),x7),(mark(count),x6),(mark(count),x5),(mark(count),x4),(mark(count),x3),(mark(count),x2),(mark(count),x1),(mark(count),x0)); __builtin_memcpy(out,&result,16); }
void construct_6(unsigned char *out,const unsigned char *input,unsigned *count) { short x0; __builtin_memcpy(&x0,input+0,2);short x1; __builtin_memcpy(&x1,input+2,2);short x2; __builtin_memcpy(&x2,input+4,2);short x3; __builtin_memcpy(&x3,input+6,2);short x4; __builtin_memcpy(&x4,input+8,2);short x5; __builtin_memcpy(&x5,input+10,2);short x6; __builtin_memcpy(&x6,input+12,2);short x7; __builtin_memcpy(&x7,input+14,2);__m128i result=_mm_setr_epi16((mark(count),x0),(mark(count),x1),(mark(count),x2),(mark(count),x3),(mark(count),x4),(mark(count),x5),(mark(count),x6),(mark(count),x7)); __builtin_memcpy(out,&result,16); }
void construct_7(unsigned char *out,const unsigned char *input,unsigned *count) { short x7; __builtin_memcpy(&x7,input+0,2);short x6; __builtin_memcpy(&x6,input+2,2);short x5; __builtin_memcpy(&x5,input+4,2);short x4; __builtin_memcpy(&x4,input+6,2);short x3; __builtin_memcpy(&x3,input+8,2);short x2; __builtin_memcpy(&x2,input+10,2);short x1; __builtin_memcpy(&x1,input+12,2);short x0; __builtin_memcpy(&x0,input+14,2);__m128i result=_mm_set_epi16((mark(count),x7),(mark(count),x6),(mark(count),x5),(mark(count),x4),(mark(count),x3),(mark(count),x2),(mark(count),x1),(mark(count),x0)); __builtin_memcpy(out,&result,16); }
void construct_8(unsigned char *out,const unsigned char *input,unsigned *count) { int x0; __builtin_memcpy(&x0,input+0,4);int x1; __builtin_memcpy(&x1,input+4,4);int x2; __builtin_memcpy(&x2,input+8,4);int x3; __builtin_memcpy(&x3,input+12,4);__m128i result=_mm_setr_epi32((mark(count),x0),(mark(count),x1),(mark(count),x2),(mark(count),x3)); __builtin_memcpy(out,&result,16); }
void construct_9(unsigned char *out,const unsigned char *input,unsigned *count) { char x0; __builtin_memcpy(&x0,input+0,1);char x1; __builtin_memcpy(&x1,input+1,1);char x2; __builtin_memcpy(&x2,input+2,1);char x3; __builtin_memcpy(&x3,input+3,1);char x4; __builtin_memcpy(&x4,input+4,1);char x5; __builtin_memcpy(&x5,input+5,1);char x6; __builtin_memcpy(&x6,input+6,1);char x7; __builtin_memcpy(&x7,input+7,1);char x8; __builtin_memcpy(&x8,input+8,1);char x9; __builtin_memcpy(&x9,input+9,1);char x10; __builtin_memcpy(&x10,input+10,1);char x11; __builtin_memcpy(&x11,input+11,1);char x12; __builtin_memcpy(&x12,input+12,1);char x13; __builtin_memcpy(&x13,input+13,1);char x14; __builtin_memcpy(&x14,input+14,1);char x15; __builtin_memcpy(&x15,input+15,1);char x16; __builtin_memcpy(&x16,input+16,1);char x17; __builtin_memcpy(&x17,input+17,1);char x18; __builtin_memcpy(&x18,input+18,1);char x19; __builtin_memcpy(&x19,input+19,1);char x20; __builtin_memcpy(&x20,input+20,1);char x21; __builtin_memcpy(&x21,input+21,1);char x22; __builtin_memcpy(&x22,input+22,1);char x23; __builtin_memcpy(&x23,input+23,1);char x24; __builtin_memcpy(&x24,input+24,1);char x25; __builtin_memcpy(&x25,input+25,1);char x26; __builtin_memcpy(&x26,input+26,1);char x27; __builtin_memcpy(&x27,input+27,1);char x28; __builtin_memcpy(&x28,input+28,1);char x29; __builtin_memcpy(&x29,input+29,1);char x30; __builtin_memcpy(&x30,input+30,1);char x31; __builtin_memcpy(&x31,input+31,1);__m256i result=_mm256_setr_epi8((mark(count),x0),(mark(count),x1),(mark(count),x2),(mark(count),x3),(mark(count),x4),(mark(count),x5),(mark(count),x6),(mark(count),x7),(mark(count),x8),(mark(count),x9),(mark(count),x10),(mark(count),x11),(mark(count),x12),(mark(count),x13),(mark(count),x14),(mark(count),x15),(mark(count),x16),(mark(count),x17),(mark(count),x18),(mark(count),x19),(mark(count),x20),(mark(count),x21),(mark(count),x22),(mark(count),x23),(mark(count),x24),(mark(count),x25),(mark(count),x26),(mark(count),x27),(mark(count),x28),(mark(count),x29),(mark(count),x30),(mark(count),x31)); __builtin_memcpy(out,&result,32); }
void construct_10(unsigned char *out,const unsigned char *input,unsigned *count) { char x31; __builtin_memcpy(&x31,input+0,1);char x30; __builtin_memcpy(&x30,input+1,1);char x29; __builtin_memcpy(&x29,input+2,1);char x28; __builtin_memcpy(&x28,input+3,1);char x27; __builtin_memcpy(&x27,input+4,1);char x26; __builtin_memcpy(&x26,input+5,1);char x25; __builtin_memcpy(&x25,input+6,1);char x24; __builtin_memcpy(&x24,input+7,1);char x23; __builtin_memcpy(&x23,input+8,1);char x22; __builtin_memcpy(&x22,input+9,1);char x21; __builtin_memcpy(&x21,input+10,1);char x20; __builtin_memcpy(&x20,input+11,1);char x19; __builtin_memcpy(&x19,input+12,1);char x18; __builtin_memcpy(&x18,input+13,1);char x17; __builtin_memcpy(&x17,input+14,1);char x16; __builtin_memcpy(&x16,input+15,1);char x15; __builtin_memcpy(&x15,input+16,1);char x14; __builtin_memcpy(&x14,input+17,1);char x13; __builtin_memcpy(&x13,input+18,1);char x12; __builtin_memcpy(&x12,input+19,1);char x11; __builtin_memcpy(&x11,input+20,1);char x10; __builtin_memcpy(&x10,input+21,1);char x9; __builtin_memcpy(&x9,input+22,1);char x8; __builtin_memcpy(&x8,input+23,1);char x7; __builtin_memcpy(&x7,input+24,1);char x6; __builtin_memcpy(&x6,input+25,1);char x5; __builtin_memcpy(&x5,input+26,1);char x4; __builtin_memcpy(&x4,input+27,1);char x3; __builtin_memcpy(&x3,input+28,1);char x2; __builtin_memcpy(&x2,input+29,1);char x1; __builtin_memcpy(&x1,input+30,1);char x0; __builtin_memcpy(&x0,input+31,1);__m256i result=_mm256_set_epi8((mark(count),x31),(mark(count),x30),(mark(count),x29),(mark(count),x28),(mark(count),x27),(mark(count),x26),(mark(count),x25),(mark(count),x24),(mark(count),x23),(mark(count),x22),(mark(count),x21),(mark(count),x20),(mark(count),x19),(mark(count),x18),(mark(count),x17),(mark(count),x16),(mark(count),x15),(mark(count),x14),(mark(count),x13),(mark(count),x12),(mark(count),x11),(mark(count),x10),(mark(count),x9),(mark(count),x8),(mark(count),x7),(mark(count),x6),(mark(count),x5),(mark(count),x4),(mark(count),x3),(mark(count),x2),(mark(count),x1),(mark(count),x0)); __builtin_memcpy(out,&result,32); }
void construct_11(unsigned char *out,const unsigned char *input,unsigned *count) { short x0; __builtin_memcpy(&x0,input+0,2);short x1; __builtin_memcpy(&x1,input+2,2);short x2; __builtin_memcpy(&x2,input+4,2);short x3; __builtin_memcpy(&x3,input+6,2);short x4; __builtin_memcpy(&x4,input+8,2);short x5; __builtin_memcpy(&x5,input+10,2);short x6; __builtin_memcpy(&x6,input+12,2);short x7; __builtin_memcpy(&x7,input+14,2);short x8; __builtin_memcpy(&x8,input+16,2);short x9; __builtin_memcpy(&x9,input+18,2);short x10; __builtin_memcpy(&x10,input+20,2);short x11; __builtin_memcpy(&x11,input+22,2);short x12; __builtin_memcpy(&x12,input+24,2);short x13; __builtin_memcpy(&x13,input+26,2);short x14; __builtin_memcpy(&x14,input+28,2);short x15; __builtin_memcpy(&x15,input+30,2);__m256i result=_mm256_setr_epi16((mark(count),x0),(mark(count),x1),(mark(count),x2),(mark(count),x3),(mark(count),x4),(mark(count),x5),(mark(count),x6),(mark(count),x7),(mark(count),x8),(mark(count),x9),(mark(count),x10),(mark(count),x11),(mark(count),x12),(mark(count),x13),(mark(count),x14),(mark(count),x15)); __builtin_memcpy(out,&result,32); }
void construct_12(unsigned char *out,const unsigned char *input,unsigned *count) { short x15; __builtin_memcpy(&x15,input+0,2);short x14; __builtin_memcpy(&x14,input+2,2);short x13; __builtin_memcpy(&x13,input+4,2);short x12; __builtin_memcpy(&x12,input+6,2);short x11; __builtin_memcpy(&x11,input+8,2);short x10; __builtin_memcpy(&x10,input+10,2);short x9; __builtin_memcpy(&x9,input+12,2);short x8; __builtin_memcpy(&x8,input+14,2);short x7; __builtin_memcpy(&x7,input+16,2);short x6; __builtin_memcpy(&x6,input+18,2);short x5; __builtin_memcpy(&x5,input+20,2);short x4; __builtin_memcpy(&x4,input+22,2);short x3; __builtin_memcpy(&x3,input+24,2);short x2; __builtin_memcpy(&x2,input+26,2);short x1; __builtin_memcpy(&x1,input+28,2);short x0; __builtin_memcpy(&x0,input+30,2);__m256i result=_mm256_set_epi16((mark(count),x15),(mark(count),x14),(mark(count),x13),(mark(count),x12),(mark(count),x11),(mark(count),x10),(mark(count),x9),(mark(count),x8),(mark(count),x7),(mark(count),x6),(mark(count),x5),(mark(count),x4),(mark(count),x3),(mark(count),x2),(mark(count),x1),(mark(count),x0)); __builtin_memcpy(out,&result,32); }
void construct_13(unsigned char *out,const unsigned char *input,unsigned *count) { int x0; __builtin_memcpy(&x0,input+0,4);int x1; __builtin_memcpy(&x1,input+4,4);int x2; __builtin_memcpy(&x2,input+8,4);int x3; __builtin_memcpy(&x3,input+12,4);int x4; __builtin_memcpy(&x4,input+16,4);int x5; __builtin_memcpy(&x5,input+20,4);int x6; __builtin_memcpy(&x6,input+24,4);int x7; __builtin_memcpy(&x7,input+28,4);__m256i result=_mm256_setr_epi32((mark(count),x0),(mark(count),x1),(mark(count),x2),(mark(count),x3),(mark(count),x4),(mark(count),x5),(mark(count),x6),(mark(count),x7)); __builtin_memcpy(out,&result,32); }
void construct_14(unsigned char *out,const unsigned char *input,unsigned *count) { long long x0; __builtin_memcpy(&x0,input+0,8);long long x1; __builtin_memcpy(&x1,input+8,8);long long x2; __builtin_memcpy(&x2,input+16,8);long long x3; __builtin_memcpy(&x3,input+24,8);__m256i result=_mm256_setr_epi64x((mark(count),x0),(mark(count),x1),(mark(count),x2),(mark(count),x3)); __builtin_memcpy(out,&result,32); }
void construct_15(unsigned char *out,const unsigned char *input,unsigned *count) { __m128 a; __builtin_memcpy(&a,input+0,16);__m128 result=_mm_movehdup_ps((mark(count),a)); __builtin_memcpy(out,&result,16); }
void construct_16(unsigned char *out,const unsigned char *input,unsigned *count) { __m128 a; __builtin_memcpy(&a,input+0,16);__m128 result=_mm_moveldup_ps((mark(count),a)); __builtin_memcpy(out,&result,16); }
void construct_17(unsigned char *out,const unsigned char *input,unsigned *count) { __m128d a; __builtin_memcpy(&a,input+0,16);__m128d result=_mm_movedup_pd((mark(count),a)); __builtin_memcpy(out,&result,16); }
void construct_18(unsigned char *out,const unsigned char *input,unsigned *count) { __m256 a; __builtin_memcpy(&a,input+0,32);__m256 result=_mm256_movehdup_ps((mark(count),a)); __builtin_memcpy(out,&result,32); }
void construct_19(unsigned char *out,const unsigned char *input,unsigned *count) { __m256 a; __builtin_memcpy(&a,input+0,32);__m256 result=_mm256_moveldup_ps((mark(count),a)); __builtin_memcpy(out,&result,32); }
void construct_20(unsigned char *out,const unsigned char *input,unsigned *count) { __m256d a; __builtin_memcpy(&a,input+0,32);__m256d result=_mm256_movedup_pd((mark(count),a)); __builtin_memcpy(out,&result,32); }
void construct_21(unsigned char *out,const unsigned char *input,unsigned *count) { __m128 a; __builtin_memcpy(&a,input+0,16);__m256 result=_mm256_zextps128_ps256((mark(count),a)); __builtin_memcpy(out,&result,32); }
void construct_22(unsigned char *out,const unsigned char *input,unsigned *count) { __m128 a; __builtin_memcpy(&a,input+0,16);__m256 result=_mm256_castps128_ps256((mark(count),a)); __builtin_memcpy(out,&result,32); }
void construct_23(unsigned char *out,const unsigned char *input,unsigned *count) { __m128 hi; __builtin_memcpy(&hi,input+0,16);__m128 lo; __builtin_memcpy(&lo,input+16,16);__m256 result=_mm256_set_m128((mark(count),hi),(mark(count),lo)); __builtin_memcpy(out,&result,32); }
void construct_24(unsigned char *out,const unsigned char *input,unsigned *count) { __m128 lo; __builtin_memcpy(&lo,input+0,16);__m128 hi; __builtin_memcpy(&hi,input+16,16);__m256 result=_mm256_setr_m128((mark(count),lo),(mark(count),hi)); __builtin_memcpy(out,&result,32); }
void construct_25(unsigned char *out,const unsigned char *input,unsigned *count) { __m128d a; __builtin_memcpy(&a,input+0,16);__m256d result=_mm256_zextpd128_pd256((mark(count),a)); __builtin_memcpy(out,&result,32); }
void construct_26(unsigned char *out,const unsigned char *input,unsigned *count) { __m128d a; __builtin_memcpy(&a,input+0,16);__m256d result=_mm256_castpd128_pd256((mark(count),a)); __builtin_memcpy(out,&result,32); }
void construct_27(unsigned char *out,const unsigned char *input,unsigned *count) { __m128d hi; __builtin_memcpy(&hi,input+0,16);__m128d lo; __builtin_memcpy(&lo,input+16,16);__m256d result=_mm256_set_m128d((mark(count),hi),(mark(count),lo)); __builtin_memcpy(out,&result,32); }
void construct_28(unsigned char *out,const unsigned char *input,unsigned *count) { __m128d lo; __builtin_memcpy(&lo,input+0,16);__m128d hi; __builtin_memcpy(&hi,input+16,16);__m256d result=_mm256_setr_m128d((mark(count),lo),(mark(count),hi)); __builtin_memcpy(out,&result,32); }
void construct_29(unsigned char *out,const unsigned char *input,unsigned *count) { __m128i a; __builtin_memcpy(&a,input+0,16);__m256i result=_mm256_zextsi128_si256((mark(count),a)); __builtin_memcpy(out,&result,32); }
void construct_30(unsigned char *out,const unsigned char *input,unsigned *count) { __m128i a; __builtin_memcpy(&a,input+0,16);__m256i result=_mm256_castsi128_si256((mark(count),a)); __builtin_memcpy(out,&result,32); }
void construct_31(unsigned char *out,const unsigned char *input,unsigned *count) { __m128i hi; __builtin_memcpy(&hi,input+0,16);__m128i lo; __builtin_memcpy(&lo,input+16,16);__m256i result=_mm256_set_m128i((mark(count),hi),(mark(count),lo)); __builtin_memcpy(out,&result,32); }
void construct_32(unsigned char *out,const unsigned char *input,unsigned *count) { __m128i lo; __builtin_memcpy(&lo,input+0,16);__m128i hi; __builtin_memcpy(&hi,input+16,16);__m256i result=_mm256_setr_m128i((mark(count),lo),(mark(count),hi)); __builtin_memcpy(out,&result,32); }
void construct_33(unsigned char *out,const unsigned char *input,unsigned *count) { __m128 a; __builtin_memcpy(&a,input+0,16);__m128 result=_mm_broadcastss_ps((mark(count),a)); __builtin_memcpy(out,&result,16); }
void construct_34(unsigned char *out,const unsigned char *input,unsigned *count) { __m128d a; __builtin_memcpy(&a,input+0,16);__m128d result=_mm_broadcastsd_pd((mark(count),a)); __builtin_memcpy(out,&result,16); }
void construct_35(unsigned char *out,const unsigned char *input,unsigned *count) { __m128i a; __builtin_memcpy(&a,input+0,16);__m128i result=_mm_broadcastb_epi8((mark(count),a)); __builtin_memcpy(out,&result,16); }
void construct_36(unsigned char *out,const unsigned char *input,unsigned *count) { __m128i a; __builtin_memcpy(&a,input+0,16);__m128i result=_mm_broadcastw_epi16((mark(count),a)); __builtin_memcpy(out,&result,16); }
void construct_37(unsigned char *out,const unsigned char *input,unsigned *count) { __m128i a; __builtin_memcpy(&a,input+0,16);__m128i result=_mm_broadcastd_epi32((mark(count),a)); __builtin_memcpy(out,&result,16); }
void construct_38(unsigned char *out,const unsigned char *input,unsigned *count) { __m128i a; __builtin_memcpy(&a,input+0,16);__m128i result=_mm_broadcastq_epi64((mark(count),a)); __builtin_memcpy(out,&result,16); }
void construct_39(unsigned char *out,const unsigned char *input,unsigned *count) { __m128 a; __builtin_memcpy(&a,input+0,16);__m256 result=_mm256_broadcastss_ps((mark(count),a)); __builtin_memcpy(out,&result,32); }
void construct_40(unsigned char *out,const unsigned char *input,unsigned *count) { __m128d a; __builtin_memcpy(&a,input+0,16);__m256d result=_mm256_broadcastsd_pd((mark(count),a)); __builtin_memcpy(out,&result,32); }
void construct_41(unsigned char *out,const unsigned char *input,unsigned *count) { __m128i a; __builtin_memcpy(&a,input+0,16);__m256i result=_mm256_broadcastb_epi8((mark(count),a)); __builtin_memcpy(out,&result,32); }
void construct_42(unsigned char *out,const unsigned char *input,unsigned *count) { __m128i a; __builtin_memcpy(&a,input+0,16);__m256i result=_mm256_broadcastw_epi16((mark(count),a)); __builtin_memcpy(out,&result,32); }
void construct_43(unsigned char *out,const unsigned char *input,unsigned *count) { __m128i a; __builtin_memcpy(&a,input+0,16);__m256i result=_mm256_broadcastd_epi32((mark(count),a)); __builtin_memcpy(out,&result,32); }
void construct_44(unsigned char *out,const unsigned char *input,unsigned *count) { __m128i a; __builtin_memcpy(&a,input+0,16);__m256i result=_mm256_broadcastq_epi64((mark(count),a)); __builtin_memcpy(out,&result,32); }
void construct_45(unsigned char *out,const unsigned char *input,unsigned *count) { __m128i a; __builtin_memcpy(&a,input+0,16);__m256i result=_mm256_broadcastsi128_si256((mark(count),a)); __builtin_memcpy(out,&result,32); }
void construct_46(unsigned char *out,const unsigned char *input,unsigned *count) { const float * p = (const void *)input;__m128 result=_mm_broadcast_ss((mark(count),p)); __builtin_memcpy(out,&result,16); }
void construct_47(unsigned char *out,const unsigned char *input,unsigned *count) { const float * p = (const void *)input;__m256 result=_mm256_broadcast_ss((mark(count),p)); __builtin_memcpy(out,&result,32); }
void construct_48(unsigned char *out,const unsigned char *input,unsigned *count) { const double * p = (const void *)input;__m256d result=_mm256_broadcast_sd((mark(count),p)); __builtin_memcpy(out,&result,32); }
void construct_49(unsigned char *out,const unsigned char *input,unsigned *count) { const __m128 * p = (const void *)input;__m256 result=_mm256_broadcast_ps((mark(count),p)); __builtin_memcpy(out,&result,32); }
void construct_50(unsigned char *out,const unsigned char *input,unsigned *count) { const __m128d * p = (const void *)input;__m256d result=_mm256_broadcast_pd((mark(count),p)); __builtin_memcpy(out,&result,32); }

void memory_0(unsigned char*out,const unsigned char*input,unsigned*count){__m128 r=_mm_load_ss((mark(count),(const float*)input));__builtin_memcpy(out,&r,16);}

void memory_1(unsigned char*out,const unsigned char*input,unsigned*count){__m128 a;__builtin_memcpy(&a,input,16);_mm_store_ss((mark(count),(float*)out),a);}

void memory_2(unsigned char*out,const unsigned char*input,unsigned*count){__m128 r=_mm_load1_ps((mark(count),(const float*)input));__builtin_memcpy(out,&r,16);}

void memory_3(unsigned char*out,const unsigned char*input,unsigned*count){__m128 r=_mm_load_ps1((mark(count),(const float*)input));__builtin_memcpy(out,&r,16);}

void memory_4(unsigned char*out,const unsigned char*input,unsigned*count){__m128 a;__builtin_memcpy(&a,input,16);_mm_store1_ps((mark(count),(float*)out),a);}

void memory_5(unsigned char*out,const unsigned char*input,unsigned*count){__m128 a;__builtin_memcpy(&a,input,16);_mm_store_ps1((mark(count),(float*)out),a);}

void memory_6(unsigned char*out,const unsigned char*input,unsigned*count){__m128 r=_mm_loadr_ps((mark(count),(const float*)input));__builtin_memcpy(out,&r,16);}

void memory_7(unsigned char*out,const unsigned char*input,unsigned*count){__m128 a;__builtin_memcpy(&a,input,16);_mm_storer_ps((mark(count),(float*)out),a);}

void memory_8(unsigned char*out,const unsigned char*input,unsigned*count){__m128 a,b;__builtin_memcpy(&a,input,16);__builtin_memcpy(&b,input+16,16);__m128 r=_mm_move_ss(a,(mark(count),b));__builtin_memcpy(out,&r,16);}

void memory_9(unsigned char*out,const unsigned char*input,unsigned*count){__m128d r=_mm_load_sd((mark(count),(const double*)input));__builtin_memcpy(out,&r,16);}

void memory_10(unsigned char*out,const unsigned char*input,unsigned*count){__m128d a;__builtin_memcpy(&a,input,16);_mm_store_sd((mark(count),(double*)out),a);}

void memory_11(unsigned char*out,const unsigned char*input,unsigned*count){__m128d r=_mm_load1_pd((mark(count),(const double*)input));__builtin_memcpy(out,&r,16);}

void memory_12(unsigned char*out,const unsigned char*input,unsigned*count){__m128d r=_mm_load_pd1((mark(count),(const double*)input));__builtin_memcpy(out,&r,16);}

void memory_13(unsigned char*out,const unsigned char*input,unsigned*count){__m128d a;__builtin_memcpy(&a,input,16);_mm_store1_pd((mark(count),(double*)out),a);}

void memory_14(unsigned char*out,const unsigned char*input,unsigned*count){__m128d a;__builtin_memcpy(&a,input,16);_mm_store_pd1((mark(count),(double*)out),a);}

void memory_15(unsigned char*out,const unsigned char*input,unsigned*count){__m128d r=_mm_loadr_pd((mark(count),(const double*)input));__builtin_memcpy(out,&r,16);}

void memory_16(unsigned char*out,const unsigned char*input,unsigned*count){__m128d a;__builtin_memcpy(&a,input,16);_mm_storer_pd((mark(count),(double*)out),a);}

void memory_17(unsigned char*out,const unsigned char*input,unsigned*count){__m128d a,b;__builtin_memcpy(&a,input,16);__builtin_memcpy(&b,input+16,16);__m128d r=_mm_move_sd(a,(mark(count),b));__builtin_memcpy(out,&r,16);}

void memory_18(unsigned char*out,const unsigned char*input,unsigned*count){__m128d a;__builtin_memcpy(&a,input,16);__m128d r=_mm_loadl_pd(a,(mark(count),(const double*)(input+16)));__builtin_memcpy(out,&r,16);}

void memory_19(unsigned char*out,const unsigned char*input,unsigned*count){__m128d a;__builtin_memcpy(&a,input,16);_mm_storel_pd((mark(count),(double*)out),a);}

void memory_20(unsigned char*out,const unsigned char*input,unsigned*count){__m128d a;__builtin_memcpy(&a,input,16);__m128d r=_mm_loadh_pd(a,(mark(count),(const double*)(input+16)));__builtin_memcpy(out,&r,16);}

void memory_21(unsigned char*out,const unsigned char*input,unsigned*count){__m128d a;__builtin_memcpy(&a,input,16);_mm_storeh_pd((mark(count),(double*)out),a);}

void memory_22(unsigned char*out,const unsigned char*input,unsigned*count){__m128i r=_mm_loadu_si16((mark(count),input));__builtin_memcpy(out,&r,16);}

void memory_23(unsigned char*out,const unsigned char*input,unsigned*count){__m128i a;__builtin_memcpy(&a,input,16);_mm_storeu_si16((mark(count),out),a);}

void memory_24(unsigned char*out,const unsigned char*input,unsigned*count){__m128i r=_mm_loadu_si32((mark(count),input));__builtin_memcpy(out,&r,16);}

void memory_25(unsigned char*out,const unsigned char*input,unsigned*count){__m128i a;__builtin_memcpy(&a,input,16);_mm_storeu_si32((mark(count),out),a);}

void memory_26(unsigned char*out,const unsigned char*input,unsigned*count){__m128i r=_mm_loadu_si64((mark(count),input));__builtin_memcpy(out,&r,16);}

void memory_27(unsigned char*out,const unsigned char*input,unsigned*count){__m128i a;__builtin_memcpy(&a,input,16);_mm_storeu_si64((mark(count),out),a);}

void memory_28(unsigned char*out,const unsigned char*input,unsigned*count){__m128i r=_mm_loadl_epi64((mark(count),(const __m128i*)input));__builtin_memcpy(out,&r,16);}

void memory_29(unsigned char*out,const unsigned char*input,unsigned*count){__m128i a;__builtin_memcpy(&a,input,16);_mm_storel_epi64((mark(count),(__m128i*)out),a);}

void memory_30(unsigned char*out,const unsigned char*input,unsigned*count){__m128d r=_mm_loaddup_pd((mark(count),(const double*)input));__builtin_memcpy(out,&r,16);}

void convenience_0(unsigned char*out,const unsigned char*input,unsigned*count) { __m128 a,b;__builtin_memcpy(&a,input,16);__builtin_memcpy(&b,input+16,16);__m128 r=_mm_movehl_ps((mark(count),a),(mark(count),b));__builtin_memcpy(out,&r,16); }

void convenience_1(unsigned char*out,const unsigned char*input,unsigned*count) { __m128 a,b;__builtin_memcpy(&a,input,16);__builtin_memcpy(&b,input+16,16);__m128 r=_mm_movelh_ps((mark(count),a),(mark(count),b));__builtin_memcpy(out,&r,16); }

void convenience_2(unsigned char*out,const unsigned char*input,unsigned*count) { float a;__builtin_memcpy(&a,input,4);__m128 r=_mm_set_ps1((mark(count),a));__builtin_memcpy(out,&r,16); }

void convenience_3(unsigned char*out,const unsigned char*input,unsigned*count) { float a;__builtin_memcpy(&a,input,4);__m128 r=_mm_set_ss((mark(count),a));__builtin_memcpy(out,&r,16); }

void convenience_4(unsigned char*out,const unsigned char*input,unsigned*count) { double a;__builtin_memcpy(&a,input,8);__m128d r=_mm_set_pd1((mark(count),a));__builtin_memcpy(out,&r,16); }

void convenience_5(unsigned char*out,const unsigned char*input,unsigned*count) { double a;__builtin_memcpy(&a,input,8);__m128d r=_mm_set_sd((mark(count),a));__builtin_memcpy(out,&r,16); }

void convenience_6(unsigned char*out,const unsigned char*input,unsigned*count) { __m128d a;__builtin_memcpy(&a,input,16);double r=_mm_cvtsd_f64((mark(count),a));__builtin_memcpy(out,&r,8); }

void convenience_7(unsigned char*out,const unsigned char*input,unsigned*count) { __m128i a;__builtin_memcpy(&a,input,16);int r=_mm_cvtsi128_si32((mark(count),a));__builtin_memcpy(out,&r,4); }

void convenience_8(unsigned char*out,const unsigned char*input,unsigned*count) { __m128i a;__builtin_memcpy(&a,input,16);long long r=_mm_cvtsi128_si64((mark(count),a));__builtin_memcpy(out,&r,8); }

void convenience_9(unsigned char*out,const unsigned char*input,unsigned*count) { int a;__builtin_memcpy(&a,input,4);__m128i r=_mm_cvtsi32_si128((mark(count),a));__builtin_memcpy(out,&r,16); }

void convenience_10(unsigned char*out,const unsigned char*input,unsigned*count) { long long a;__builtin_memcpy(&a,input,8);__m128i r=_mm_cvtsi64_si128((mark(count),a));__builtin_memcpy(out,&r,16); }

void convenience_11(unsigned char*out,const unsigned char*input,unsigned*count) { __m128i a;__builtin_memcpy(&a,input,16);__m128i r=_mm_move_epi64((mark(count),a));__builtin_memcpy(out,&r,16); }

void convenience_12(unsigned char*out,const unsigned char*input,unsigned*count) { __m128 r=_mm_undefined_ps();__builtin_memcpy(out,&r,16); }

void convenience_13(unsigned char*out,const unsigned char*input,unsigned*count) { __m128d r=_mm_undefined_pd();__builtin_memcpy(out,&r,16); }

void convenience_14(unsigned char*out,const unsigned char*input,unsigned*count) { __m128i r=_mm_undefined_si128();__builtin_memcpy(out,&r,16); }

void convenience_15(unsigned char*out,const unsigned char*input,unsigned*count) { __m256 r=_mm256_undefined_ps();__builtin_memcpy(out,&r,32); }

void convenience_16(unsigned char*out,const unsigned char*input,unsigned*count) { __m256d r=_mm256_undefined_pd();__builtin_memcpy(out,&r,32); }

void convenience_17(unsigned char*out,const unsigned char*input,unsigned*count) { __m256i r=_mm256_undefined_si256();__builtin_memcpy(out,&r,32); }

void convenience_18(unsigned char*out,const unsigned char*input,unsigned*count) { __m128i r=_mm_lddqu_si128((mark(count),(const __m128i*)input));__builtin_memcpy(out,&r,16); }

void convenience_19(unsigned char*out,const unsigned char*input,unsigned*count) { __m256i r=_mm256_lddqu_si256((mark(count),(const __m256i*)input));__builtin_memcpy(out,&r,32); }

void convenience_20(unsigned char*out,const unsigned char*input,unsigned*count) { __m256 a;__builtin_memcpy(&a,input,32);float r=_mm256_cvtss_f32((mark(count),a));__builtin_memcpy(out,&r,4); }

void convenience_21(unsigned char*out,const unsigned char*input,unsigned*count) { __m256d a;__builtin_memcpy(&a,input,32);double r=_mm256_cvtsd_f64((mark(count),a));__builtin_memcpy(out,&r,8); }

void convenience_22(unsigned char*out,const unsigned char*input,unsigned*count) { __m256i a;__builtin_memcpy(&a,input,32);int r=_mm256_cvtsi256_si32((mark(count),a));__builtin_memcpy(out,&r,4); }

void convenience_23(unsigned char*out,const unsigned char*input,unsigned*count) { __m128i a;__builtin_memcpy(&a,input,16);int r=_mm_extract_epi8((mark(count),a),0);__builtin_memcpy(out,&r,4); }

void convenience_24(unsigned char*out,const unsigned char*input,unsigned*count) { __m128i a;int x;__builtin_memcpy(&a,input,16);__builtin_memcpy(&x,input+16,4);__m128i r=_mm_insert_epi8((mark(count),a),(mark(count),x),0);__builtin_memcpy(out,&r,16); }

void convenience_25(unsigned char*out,const unsigned char*input,unsigned*count) { __m128i a;__builtin_memcpy(&a,input,16);int r=_mm_extract_epi8((mark(count),a),15);__builtin_memcpy(out,&r,4); }

void convenience_26(unsigned char*out,const unsigned char*input,unsigned*count) { __m128i a;int x;__builtin_memcpy(&a,input,16);__builtin_memcpy(&x,input+16,4);__m128i r=_mm_insert_epi8((mark(count),a),(mark(count),x),15);__builtin_memcpy(out,&r,16); }

void convenience_27(unsigned char*out,const unsigned char*input,unsigned*count) { __m128i a;__builtin_memcpy(&a,input,16);int r=_mm_extract_epi8((mark(count),a),17);__builtin_memcpy(out,&r,4); }

void convenience_28(unsigned char*out,const unsigned char*input,unsigned*count) { __m128i a;int x;__builtin_memcpy(&a,input,16);__builtin_memcpy(&x,input+16,4);__m128i r=_mm_insert_epi8((mark(count),a),(mark(count),x),17);__builtin_memcpy(out,&r,16); }

void convenience_29(unsigned char*out,const unsigned char*input,unsigned*count) { __m128i a;__builtin_memcpy(&a,input,16);long long r=_mm_extract_epi64((mark(count),a),0);__builtin_memcpy(out,&r,8); }

void convenience_30(unsigned char*out,const unsigned char*input,unsigned*count) { __m128i a;long long x;__builtin_memcpy(&a,input,16);__builtin_memcpy(&x,input+16,8);__m128i r=_mm_insert_epi64((mark(count),a),(mark(count),x),0);__builtin_memcpy(out,&r,16); }

void convenience_31(unsigned char*out,const unsigned char*input,unsigned*count) { __m128i a;__builtin_memcpy(&a,input,16);long long r=_mm_extract_epi64((mark(count),a),1);__builtin_memcpy(out,&r,8); }

void convenience_32(unsigned char*out,const unsigned char*input,unsigned*count) { __m128i a;long long x;__builtin_memcpy(&a,input,16);__builtin_memcpy(&x,input+16,8);__m128i r=_mm_insert_epi64((mark(count),a),(mark(count),x),1);__builtin_memcpy(out,&r,16); }

void convenience_33(unsigned char*out,const unsigned char*input,unsigned*count) { __m128i a;__builtin_memcpy(&a,input,16);long long r=_mm_extract_epi64((mark(count),a),3);__builtin_memcpy(out,&r,8); }

void convenience_34(unsigned char*out,const unsigned char*input,unsigned*count) { __m128i a;long long x;__builtin_memcpy(&a,input,16);__builtin_memcpy(&x,input+16,8);__m128i r=_mm_insert_epi64((mark(count),a),(mark(count),x),3);__builtin_memcpy(out,&r,16); }

void convenience_35(unsigned char*out,const unsigned char*input,unsigned*count) { __m256i a;__builtin_memcpy(&a,input,32);int r=_mm256_extract_epi8((mark(count),a),0);__builtin_memcpy(out,&r,4); }

void convenience_36(unsigned char*out,const unsigned char*input,unsigned*count) { __m256i a;int x;__builtin_memcpy(&a,input,32);__builtin_memcpy(&x,input+32,4);__m256i r=_mm256_insert_epi8((mark(count),a),(mark(count),x),0);__builtin_memcpy(out,&r,32); }

void convenience_37(unsigned char*out,const unsigned char*input,unsigned*count) { __m256i a;__builtin_memcpy(&a,input,32);int r=_mm256_extract_epi8((mark(count),a),31);__builtin_memcpy(out,&r,4); }

void convenience_38(unsigned char*out,const unsigned char*input,unsigned*count) { __m256i a;int x;__builtin_memcpy(&a,input,32);__builtin_memcpy(&x,input+32,4);__m256i r=_mm256_insert_epi8((mark(count),a),(mark(count),x),31);__builtin_memcpy(out,&r,32); }

void convenience_39(unsigned char*out,const unsigned char*input,unsigned*count) { __m256i a;__builtin_memcpy(&a,input,32);int r=_mm256_extract_epi8((mark(count),a),33);__builtin_memcpy(out,&r,4); }

void convenience_40(unsigned char*out,const unsigned char*input,unsigned*count) { __m256i a;int x;__builtin_memcpy(&a,input,32);__builtin_memcpy(&x,input+32,4);__m256i r=_mm256_insert_epi8((mark(count),a),(mark(count),x),33);__builtin_memcpy(out,&r,32); }

void convenience_41(unsigned char*out,const unsigned char*input,unsigned*count) { __m256i a;__builtin_memcpy(&a,input,32);int r=_mm256_extract_epi16((mark(count),a),0);__builtin_memcpy(out,&r,4); }

void convenience_42(unsigned char*out,const unsigned char*input,unsigned*count) { __m256i a;int x;__builtin_memcpy(&a,input,32);__builtin_memcpy(&x,input+32,4);__m256i r=_mm256_insert_epi16((mark(count),a),(mark(count),x),0);__builtin_memcpy(out,&r,32); }

void convenience_43(unsigned char*out,const unsigned char*input,unsigned*count) { __m256i a;__builtin_memcpy(&a,input,32);int r=_mm256_extract_epi16((mark(count),a),15);__builtin_memcpy(out,&r,4); }

void convenience_44(unsigned char*out,const unsigned char*input,unsigned*count) { __m256i a;int x;__builtin_memcpy(&a,input,32);__builtin_memcpy(&x,input+32,4);__m256i r=_mm256_insert_epi16((mark(count),a),(mark(count),x),15);__builtin_memcpy(out,&r,32); }

void convenience_45(unsigned char*out,const unsigned char*input,unsigned*count) { __m256i a;__builtin_memcpy(&a,input,32);int r=_mm256_extract_epi16((mark(count),a),17);__builtin_memcpy(out,&r,4); }

void convenience_46(unsigned char*out,const unsigned char*input,unsigned*count) { __m256i a;int x;__builtin_memcpy(&a,input,32);__builtin_memcpy(&x,input+32,4);__m256i r=_mm256_insert_epi16((mark(count),a),(mark(count),x),17);__builtin_memcpy(out,&r,32); }

void convenience_47(unsigned char*out,const unsigned char*input,unsigned*count) { __m256i a;__builtin_memcpy(&a,input,32);int r=_mm256_extract_epi32((mark(count),a),0);__builtin_memcpy(out,&r,4); }

void convenience_48(unsigned char*out,const unsigned char*input,unsigned*count) { __m256i a;int x;__builtin_memcpy(&a,input,32);__builtin_memcpy(&x,input+32,4);__m256i r=_mm256_insert_epi32((mark(count),a),(mark(count),x),0);__builtin_memcpy(out,&r,32); }

void convenience_49(unsigned char*out,const unsigned char*input,unsigned*count) { __m256i a;__builtin_memcpy(&a,input,32);int r=_mm256_extract_epi32((mark(count),a),7);__builtin_memcpy(out,&r,4); }

void convenience_50(unsigned char*out,const unsigned char*input,unsigned*count) { __m256i a;int x;__builtin_memcpy(&a,input,32);__builtin_memcpy(&x,input+32,4);__m256i r=_mm256_insert_epi32((mark(count),a),(mark(count),x),7);__builtin_memcpy(out,&r,32); }

void convenience_51(unsigned char*out,const unsigned char*input,unsigned*count) { __m256i a;__builtin_memcpy(&a,input,32);int r=_mm256_extract_epi32((mark(count),a),9);__builtin_memcpy(out,&r,4); }

void convenience_52(unsigned char*out,const unsigned char*input,unsigned*count) { __m256i a;int x;__builtin_memcpy(&a,input,32);__builtin_memcpy(&x,input+32,4);__m256i r=_mm256_insert_epi32((mark(count),a),(mark(count),x),9);__builtin_memcpy(out,&r,32); }

void convenience_53(unsigned char*out,const unsigned char*input,unsigned*count) { __m256i a;__builtin_memcpy(&a,input,32);long long r=_mm256_extract_epi64((mark(count),a),0);__builtin_memcpy(out,&r,8); }

void convenience_54(unsigned char*out,const unsigned char*input,unsigned*count) { __m256i a;long long x;__builtin_memcpy(&a,input,32);__builtin_memcpy(&x,input+32,8);__m256i r=_mm256_insert_epi64((mark(count),a),(mark(count),x),0);__builtin_memcpy(out,&r,32); }

void convenience_55(unsigned char*out,const unsigned char*input,unsigned*count) { __m256i a;__builtin_memcpy(&a,input,32);long long r=_mm256_extract_epi64((mark(count),a),3);__builtin_memcpy(out,&r,8); }

void convenience_56(unsigned char*out,const unsigned char*input,unsigned*count) { __m256i a;long long x;__builtin_memcpy(&a,input,32);__builtin_memcpy(&x,input+32,8);__m256i r=_mm256_insert_epi64((mark(count),a),(mark(count),x),3);__builtin_memcpy(out,&r,32); }

void convenience_57(unsigned char*out,const unsigned char*input,unsigned*count) { __m256i a;__builtin_memcpy(&a,input,32);long long r=_mm256_extract_epi64((mark(count),a),5);__builtin_memcpy(out,&r,8); }

void convenience_58(unsigned char*out,const unsigned char*input,unsigned*count) { __m256i a;long long x;__builtin_memcpy(&a,input,32);__builtin_memcpy(&x,input+32,8);__m256i r=_mm256_insert_epi64((mark(count),a),(mark(count),x),5);__builtin_memcpy(out,&r,32); }

void convenience_59(unsigned char*out,const unsigned char*input,unsigned*count) { __m128 a;__builtin_memcpy(&a,input,16);int r=_mm_extract_ps((mark(count),a),0);__builtin_memcpy(out,&r,4); }

void convenience_60(unsigned char*out,const unsigned char*input,unsigned*count) { __m128 a;__builtin_memcpy(&a,input,16);int r=_mm_extract_ps((mark(count),a),3);__builtin_memcpy(out,&r,4); }

void convenience_61(unsigned char*out,const unsigned char*input,unsigned*count) { __m128 a;__builtin_memcpy(&a,input,16);int r=_mm_extract_ps((mark(count),a),5);__builtin_memcpy(out,&r,4); }

void convenience_62(unsigned char*out,const unsigned char*input,unsigned*count) { __m128 a,b;__builtin_memcpy(&a,input,16);__builtin_memcpy(&b,input+16,16);__m128 r=_mm_insert_ps((mark(count),a),(mark(count),b),0);__builtin_memcpy(out,&r,16); }

void convenience_63(unsigned char*out,const unsigned char*input,unsigned*count) { __m128 a,b;__builtin_memcpy(&a,input,16);__builtin_memcpy(&b,input+16,16);__m128 r=_mm_insert_ps((mark(count),a),(mark(count),b),16);__builtin_memcpy(out,&r,16); }

void convenience_64(unsigned char*out,const unsigned char*input,unsigned*count) { __m128 a,b;__builtin_memcpy(&a,input,16);__builtin_memcpy(&b,input+16,16);__m128 r=_mm_insert_ps((mark(count),a),(mark(count),b),64);__builtin_memcpy(out,&r,16); }

void convenience_65(unsigned char*out,const unsigned char*input,unsigned*count) { __m128 a,b;__builtin_memcpy(&a,input,16);__builtin_memcpy(&b,input+16,16);__m128 r=_mm_insert_ps((mark(count),a),(mark(count),b),147);__builtin_memcpy(out,&r,16); }

void convenience_66(unsigned char*out,const unsigned char*input,unsigned*count) { __m128 a,b;__builtin_memcpy(&a,input,16);__builtin_memcpy(&b,input+16,16);__m128 r=_mm_insert_ps((mark(count),a),(mark(count),b),232);__builtin_memcpy(out,&r,16); }

void convenience_67(unsigned char*out,const unsigned char*input,unsigned*count) { __m128 a,b;__builtin_memcpy(&a,input,16);__builtin_memcpy(&b,input+16,16);__m128 r=_mm_insert_ps((mark(count),a),(mark(count),b),255);__builtin_memcpy(out,&r,16); }

void convenience_68(unsigned char*out,const unsigned char*input,unsigned*count) { __m256 r=_mm256_loadu2_m128((mark(count),(const float*)input),(mark(count),(const float*)(input+16)));__builtin_memcpy(out,&r,32); }

void convenience_69(unsigned char*out,const unsigned char*input,unsigned*count) { __m256 a;__builtin_memcpy(&a,input,32);_mm256_storeu2_m128((mark(count),(float*)out),(mark(count),(float*)(out+16)),(mark(count),a)); }

void convenience_70(unsigned char*out,const unsigned char*input,unsigned*count) { __m256d r=_mm256_loadu2_m128d((mark(count),(const double*)input),(mark(count),(const double*)(input+16)));__builtin_memcpy(out,&r,32); }

void convenience_71(unsigned char*out,const unsigned char*input,unsigned*count) { __m256d a;__builtin_memcpy(&a,input,32);_mm256_storeu2_m128d((mark(count),(double*)out),(mark(count),(double*)(out+16)),(mark(count),a)); }

void convenience_72(unsigned char*out,const unsigned char*input,unsigned*count) { __m256i r=_mm256_loadu2_m128i((mark(count),(const __m128i*)input),(mark(count),(const __m128i*)(input+16)));__builtin_memcpy(out,&r,32); }

void convenience_73(unsigned char*out,const unsigned char*input,unsigned*count) { __m256i a;__builtin_memcpy(&a,input,32);_mm256_storeu2_m128i((mark(count),(__m128i*)out),(mark(count),(__m128i*)(out+16)),(mark(count),a)); }

void convenience_compare(unsigned char*out,const unsigned char*input,unsigned*count) {
  __m128i a,b;__builtin_memcpy(&a,input,16);__builtin_memcpy(&b,input+16,16);
  __m128i r=_mm_cmplt_epi8((mark(count),a),(mark(count),b));__builtin_memcpy(out,&r,16);
  r=_mm_cmplt_epi16((mark(count),a),(mark(count),b));__builtin_memcpy(out+16,&r,16);
  r=_mm_cmplt_epi32((mark(count),a),(mark(count),b));__builtin_memcpy(out+32,&r,16);
}

void convenience_split_memory(unsigned kind,unsigned char*out_hi,unsigned char*out_lo,const unsigned char*in_hi,const unsigned char*in_lo) {
  if(kind==0) {
    __m256 a=_mm256_loadu2_m128((const float*)in_hi,(const float*)in_lo);
    _mm256_storeu2_m128((float*)out_hi,(float*)out_lo,a);
  } else if(kind==1) {
    __m256d a=_mm256_loadu2_m128d((const double*)in_hi,(const double*)in_lo);
    _mm256_storeu2_m128d((double*)out_hi,(double*)out_lo,a);
  } else {
    __m256i a=_mm256_loadu2_m128i((const __m128i*)in_hi,(const __m128i*)in_lo);
    _mm256_storeu2_m128i((__m128i*)out_hi,(__m128i*)out_lo,a);
  }
}

void alias_copy___m128(unsigned char*out,const unsigned char*input,unsigned*count) { __m128 a=*(const __m128_u*)(mark(count),input); *(__m128_u*)(mark(count),out)=a; }

void alias_copy___m128d(unsigned char*out,const unsigned char*input,unsigned*count) { __m128d a=*(const __m128d_u*)(mark(count),input); *(__m128d_u*)(mark(count),out)=a; }

void alias_copy___m128i(unsigned char*out,const unsigned char*input,unsigned*count) { __m128i a=*(const __m128i_u*)(mark(count),input); *(__m128i_u*)(mark(count),out)=a; }

void alias_copy___m256(unsigned char*out,const unsigned char*input,unsigned*count) { __m256 a=*(const __m256_u*)(mark(count),input); *(__m256_u*)(mark(count),out)=a; }

void alias_copy___m256d(unsigned char*out,const unsigned char*input,unsigned*count) { __m256d a=*(const __m256d_u*)(mark(count),input); *(__m256d_u*)(mark(count),out)=a; }

void alias_copy___m256i(unsigned char*out,const unsigned char*input,unsigned*count) { __m256i a=*(const __m256i_u*)(mark(count),input); *(__m256i_u*)(mark(count),out)=a; }
