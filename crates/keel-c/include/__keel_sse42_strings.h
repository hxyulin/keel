/* SSE4.2 explicit/implicit length comparisons use architectural IR. */
#ifndef _KEEL_SSE42_STRINGS_H
#define _KEEL_SSE42_STRINGS_H
#define _SIDD_UBYTE_OPS 0
#define _SIDD_UWORD_OPS 1
#define _SIDD_SBYTE_OPS 2
#define _SIDD_SWORD_OPS 3
#define _SIDD_CMP_EQUAL_ANY 0
#define _SIDD_CMP_RANGES 4
#define _SIDD_CMP_EQUAL_EACH 8
#define _SIDD_CMP_EQUAL_ORDERED 12
#define _SIDD_POSITIVE_POLARITY 0
#define _SIDD_NEGATIVE_POLARITY 16
#define _SIDD_MASKED_POSITIVE_POLARITY 32
#define _SIDD_MASKED_NEGATIVE_POLARITY 48
#define _SIDD_LEAST_SIGNIFICANT 0
#define _SIDD_MOST_SIGNIFICANT 64
#define _SIDD_BIT_MASK 0
#define _SIDD_UNIT_MASK 64
#define _mm_cmpistri(a,b,mode) (__builtin_keel_string_sse42pcmpistriindex((__keel_i8_128)(__m128i)(a),(__keel_i8_128)(__m128i)(b),(mode)))
#define _mm_cmpistrm(a,b,mode) ((__m128i)__builtin_keel_string_sse42pcmpistrmmask((__keel_i8_128)(__m128i)(a),(__keel_i8_128)(__m128i)(b),(mode)))
#define _mm_cmpistra(a,b,mode) __extension__ ({ int __keel_flags = __builtin_keel_string_sse42pcmpistriflags((__keel_i8_128)(__m128i)(a),(__keel_i8_128)(__m128i)(b),(mode)); (__keel_flags & 65) == 0; })
#define _mm_cmpistrc(a,b,mode) __extension__ ({ int __keel_flags = __builtin_keel_string_sse42pcmpistriflags((__keel_i8_128)(__m128i)(a),(__keel_i8_128)(__m128i)(b),(mode)); __keel_flags & 1; })
#define _mm_cmpistro(a,b,mode) __extension__ ({ int __keel_flags = __builtin_keel_string_sse42pcmpistriflags((__keel_i8_128)(__m128i)(a),(__keel_i8_128)(__m128i)(b),(mode)); (__keel_flags >> 11) & 1; })
#define _mm_cmpistrs(a,b,mode) __extension__ ({ int __keel_flags = __builtin_keel_string_sse42pcmpistriflags((__keel_i8_128)(__m128i)(a),(__keel_i8_128)(__m128i)(b),(mode)); (__keel_flags >> 7) & 1; })
#define _mm_cmpistrz(a,b,mode) __extension__ ({ int __keel_flags = __builtin_keel_string_sse42pcmpistriflags((__keel_i8_128)(__m128i)(a),(__keel_i8_128)(__m128i)(b),(mode)); (__keel_flags >> 6) & 1; })
#define _mm_cmpestri(a,la,b,lb,mode) (__builtin_keel_string_sse42pcmpestriindex((__keel_i8_128)(__m128i)(a),(__keel_i8_128)(__m128i)(b), (int)(la),(int)(lb),(mode)))
#define _mm_cmpestrm(a,la,b,lb,mode) ((__m128i)__builtin_keel_string_sse42pcmpestrmmask((__keel_i8_128)(__m128i)(a),(__keel_i8_128)(__m128i)(b), (int)(la),(int)(lb),(mode)))
#define _mm_cmpestra(a,la,b,lb,mode) __extension__ ({ int __keel_flags = __builtin_keel_string_sse42pcmpestriflags((__keel_i8_128)(__m128i)(a),(__keel_i8_128)(__m128i)(b), (int)(la),(int)(lb),(mode)); (__keel_flags & 65) == 0; })
#define _mm_cmpestrc(a,la,b,lb,mode) __extension__ ({ int __keel_flags = __builtin_keel_string_sse42pcmpestriflags((__keel_i8_128)(__m128i)(a),(__keel_i8_128)(__m128i)(b), (int)(la),(int)(lb),(mode)); __keel_flags & 1; })
#define _mm_cmpestro(a,la,b,lb,mode) __extension__ ({ int __keel_flags = __builtin_keel_string_sse42pcmpestriflags((__keel_i8_128)(__m128i)(a),(__keel_i8_128)(__m128i)(b), (int)(la),(int)(lb),(mode)); (__keel_flags >> 11) & 1; })
#define _mm_cmpestrs(a,la,b,lb,mode) __extension__ ({ int __keel_flags = __builtin_keel_string_sse42pcmpestriflags((__keel_i8_128)(__m128i)(a),(__keel_i8_128)(__m128i)(b), (int)(la),(int)(lb),(mode)); (__keel_flags >> 7) & 1; })
#define _mm_cmpestrz(a,la,b,lb,mode) __extension__ ({ int __keel_flags = __builtin_keel_string_sse42pcmpestriflags((__keel_i8_128)(__m128i)(a),(__keel_i8_128)(__m128i)(b), (int)(la),(int)(lb),(mode)); (__keel_flags >> 6) & 1; })
#endif /* _KEEL_SSE42_STRINGS_H */
