/* DQ byte masks and byte/word addition. Existing foundation word APIs stay separate. */
#ifndef _KEEL_AVX512_DQ_MASK_ALU_H
#define _KEEL_AVX512_DQ_MASK_ALU_H
#define _kand_mask8(a,b) ((__mmask8)__builtin_keel_avx512dq_kandb((signed char)(a),(signed char)(b)))
#define _kandn_mask8(a,b) ((__mmask8)__builtin_keel_avx512dq_kandnb((signed char)(a),(signed char)(b)))
#define _kor_mask8(a,b) ((__mmask8)__builtin_keel_avx512dq_korb((signed char)(a),(signed char)(b)))
#define _kxor_mask8(a,b) ((__mmask8)__builtin_keel_avx512dq_kxorb((signed char)(a),(signed char)(b)))
#define _kxnor_mask8(a,b) ((__mmask8)__builtin_keel_avx512dq_kxnorb((signed char)(a),(signed char)(b)))
#define _knot_mask8(a) ((__mmask8)__builtin_keel_avx512dq_knotb((signed char)(a)))
#define _kshiftli_mask8(a,imm) ((__mmask8)__builtin_keel_avx512dq_kshiftlb((signed char)(a),(unsigned int)(imm)))
#define _kshiftri_mask8(a,imm) ((__mmask8)__builtin_keel_avx512dq_kshiftrb((signed char)(a),(unsigned int)(imm)))
#define _kadd_mask8(a,b) ((__mmask8)__builtin_keel_avx512dq_kaddb((signed char)(a),(signed char)(b)))
#define _kadd_mask16(a,b) ((__mmask16)__builtin_keel_avx512dq_kaddw((short)(a),(short)(b)))
#define _ktest_mask8_u8(a,b,carry) ({ __mmask8 __keel_dq_a=(a),__keel_dq_b=(b); unsigned char *__keel_dq_c=(carry); unsigned char __keel_dq_flags=(unsigned char)__builtin_keel_avx512dq_ktestb((signed char)__keel_dq_a,(signed char)__keel_dq_b); *__keel_dq_c=(__keel_dq_flags>>1)&1; (unsigned char)(__keel_dq_flags&1); })
#define _ktestz_mask8_u8(a,b) ((unsigned char)((unsigned char)__builtin_keel_avx512dq_ktestb((signed char)(a),(signed char)(b))&1))
#define _ktestc_mask8_u8(a,b) ((unsigned char)(((unsigned char)__builtin_keel_avx512dq_ktestb((signed char)(a),(signed char)(b))>>1)&1))
#define _kortest_mask8_u8(a,b,carry) ({ __mmask8 __keel_dq_a=(a),__keel_dq_b=(b); unsigned char *__keel_dq_c=(carry); unsigned char __keel_dq_flags=(unsigned char)__builtin_keel_avx512dq_kortestb((signed char)__keel_dq_a,(signed char)__keel_dq_b); *__keel_dq_c=(__keel_dq_flags>>1)&1; (unsigned char)(__keel_dq_flags&1); })
#define _kortestz_mask8_u8(a,b) ((unsigned char)((unsigned char)__builtin_keel_avx512dq_kortestb((signed char)(a),(signed char)(b))&1))
#define _kortestc_mask8_u8(a,b) ((unsigned char)(((unsigned char)__builtin_keel_avx512dq_kortestb((signed char)(a),(signed char)(b))>>1)&1))
#endif
