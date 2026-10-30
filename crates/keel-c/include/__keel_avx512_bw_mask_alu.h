/* BW dword/qword mask algebra. Scalar conveniences remain baseline legal. */
#ifndef _KEEL_AVX512_BW_MASK_ALU_H
#define _KEEL_AVX512_BW_MASK_ALU_H
#define _kand_mask32(a,b) ((__mmask32)__builtin_keel_avx512bw_kandd((int)(a),(int)(b)))
#define _kandn_mask32(a,b) ((__mmask32)__builtin_keel_avx512bw_kandnd((int)(a),(int)(b)))
#define _kor_mask32(a,b) ((__mmask32)__builtin_keel_avx512bw_kord((int)(a),(int)(b)))
#define _kxor_mask32(a,b) ((__mmask32)__builtin_keel_avx512bw_kxord((int)(a),(int)(b)))
#define _kxnor_mask32(a,b) ((__mmask32)__builtin_keel_avx512bw_kxnord((int)(a),(int)(b)))
#define _kadd_mask32(a,b) ((__mmask32)__builtin_keel_avx512bw_kaddd((int)(a),(int)(b)))
#define _knot_mask32(a) ((__mmask32)__builtin_keel_avx512bw_knotd((int)(a)))
#define _kshiftli_mask32(a,imm) ((__mmask32)__builtin_keel_avx512bw_kshiftld((int)(a),(unsigned int)(imm)))
#define _kshiftri_mask32(a,imm) ((__mmask32)__builtin_keel_avx512bw_kshiftrd((int)(a),(unsigned int)(imm)))
#define _mm512_kunpackw(a,b) ((__mmask32)__builtin_keel_avx512bw_kunpckwd((int)(a),(int)(b)))
#define _ktest_mask32_u8(a,b,carry) ({ __mmask32 __keel_bw_a=(a),__keel_bw_b=(b); unsigned char *__keel_bw_c=(carry); unsigned char __keel_bw_flags=(unsigned char)__builtin_keel_avx512bw_ktestd((int)__keel_bw_a,(int)__keel_bw_b); *__keel_bw_c=(__keel_bw_flags>>1)&1; (unsigned char)(__keel_bw_flags&1); })
#define _ktestz_mask32_u8(a,b) ((unsigned char)((unsigned char)__builtin_keel_avx512bw_ktestd((int)(a),(int)(b))&1))
#define _ktestc_mask32_u8(a,b) ((unsigned char)((unsigned char)__builtin_keel_avx512bw_ktestd((int)(a),(int)(b))>>1))
#define _kortest_mask32_u8(a,b,carry) ({ __mmask32 __keel_bw_a=(a),__keel_bw_b=(b); unsigned char *__keel_bw_c=(carry); unsigned char __keel_bw_flags=(unsigned char)__builtin_keel_avx512bw_kortestd((int)__keel_bw_a,(int)__keel_bw_b); *__keel_bw_c=(__keel_bw_flags>>1)&1; (unsigned char)(__keel_bw_flags&1); })
#define _kortestz_mask32_u8(a,b) ((unsigned char)((unsigned char)__builtin_keel_avx512bw_kortestd((int)(a),(int)(b))&1))
#define _kortestc_mask32_u8(a,b) ((unsigned char)((unsigned char)__builtin_keel_avx512bw_kortestd((int)(a),(int)(b))>>1))
static __inline__ unsigned int _cvtmask32_u32(__mmask32 a) { return (unsigned int)a; }
static __inline__ __mmask32 _cvtu32_mask32(unsigned int a) { return (__mmask32)a; }
static __inline__ __mmask32 _load_mask32(__mmask32 *p) { return *p; }
static __inline__ void _store_mask32(__mmask32 *p, __mmask32 a) { *p=a; }
#define _kand_mask64(a,b) ((__mmask64)__builtin_keel_avx512bw_kandq((long long)(a),(long long)(b)))
#define _kandn_mask64(a,b) ((__mmask64)__builtin_keel_avx512bw_kandnq((long long)(a),(long long)(b)))
#define _kor_mask64(a,b) ((__mmask64)__builtin_keel_avx512bw_korq((long long)(a),(long long)(b)))
#define _kxor_mask64(a,b) ((__mmask64)__builtin_keel_avx512bw_kxorq((long long)(a),(long long)(b)))
#define _kxnor_mask64(a,b) ((__mmask64)__builtin_keel_avx512bw_kxnorq((long long)(a),(long long)(b)))
#define _kadd_mask64(a,b) ((__mmask64)__builtin_keel_avx512bw_kaddq((long long)(a),(long long)(b)))
#define _knot_mask64(a) ((__mmask64)__builtin_keel_avx512bw_knotq((long long)(a)))
#define _kshiftli_mask64(a,imm) ((__mmask64)__builtin_keel_avx512bw_kshiftlq((long long)(a),(unsigned int)(imm)))
#define _kshiftri_mask64(a,imm) ((__mmask64)__builtin_keel_avx512bw_kshiftrq((long long)(a),(unsigned int)(imm)))
#define _mm512_kunpackd(a,b) ((__mmask64)__builtin_keel_avx512bw_kunpckdq((long long)(a),(long long)(b)))
#define _ktest_mask64_u8(a,b,carry) ({ __mmask64 __keel_bw_a=(a),__keel_bw_b=(b); unsigned char *__keel_bw_c=(carry); unsigned char __keel_bw_flags=(unsigned char)__builtin_keel_avx512bw_ktestq((long long)__keel_bw_a,(long long)__keel_bw_b); *__keel_bw_c=(__keel_bw_flags>>1)&1; (unsigned char)(__keel_bw_flags&1); })
#define _ktestz_mask64_u8(a,b) ((unsigned char)((unsigned char)__builtin_keel_avx512bw_ktestq((long long)(a),(long long)(b))&1))
#define _ktestc_mask64_u8(a,b) ((unsigned char)((unsigned char)__builtin_keel_avx512bw_ktestq((long long)(a),(long long)(b))>>1))
#define _kortest_mask64_u8(a,b,carry) ({ __mmask64 __keel_bw_a=(a),__keel_bw_b=(b); unsigned char *__keel_bw_c=(carry); unsigned char __keel_bw_flags=(unsigned char)__builtin_keel_avx512bw_kortestq((long long)__keel_bw_a,(long long)__keel_bw_b); *__keel_bw_c=(__keel_bw_flags>>1)&1; (unsigned char)(__keel_bw_flags&1); })
#define _kortestz_mask64_u8(a,b) ((unsigned char)((unsigned char)__builtin_keel_avx512bw_kortestq((long long)(a),(long long)(b))&1))
#define _kortestc_mask64_u8(a,b) ((unsigned char)((unsigned char)__builtin_keel_avx512bw_kortestq((long long)(a),(long long)(b))>>1))
static __inline__ unsigned long long _cvtmask64_u64(__mmask64 a) { return (unsigned long long)a; }
static __inline__ __mmask64 _cvtu64_mask64(unsigned long long a) { return (__mmask64)a; }
static __inline__ __mmask64 _load_mask64(__mmask64 *p) { return *p; }
static __inline__ void _store_mask64(__mmask64 *p, __mmask64 a) { *p=a; }
#endif
