/* Independent declaration oracle for the private evaluator tests. No headers,
 * objects, backend execution or production Wasm C selection are required.
 * clang --target=wasm32-unknown-unknown -std=gnu11 -ffreestanding -nostdinc
 *   -Werror -fsyntax-only wasm-integer-constants.c
 */
#define TYPE(e, t) _Static_assert(__builtin_types_compatible_p(__typeof__(e), t), #e)
#define VALUE(e, v) _Static_assert((e) == (v), #e)
TYPE(2147483648, long long);
TYPE(0xffffffffL, unsigned long);
TYPE(0x100000000L, long long);
TYPE(4294967296UL, unsigned long long);
TYPE(1U + -2L, unsigned long);
TYPE(0xffffffffUL + 1LL, long long);
VALUE(1U + -2L, 4294967295ULL);
VALUE((long)(unsigned long)0xffffffffULL, -1);
VALUE((unsigned long)-1L / 2UL, 2147483647);
VALUE((long)0x100000001ULL + (long)0xffffffffULL, 0);
VALUE(((long)0x80000000ULL >> 1) < 0, 1);
VALUE(((unsigned long)-1L >> 31) == 1, 1);
VALUE((long)0x80000000ULL < 0 ? (long)0xffffffffULL : 7L, -1);
VALUE(__builtin_choose_expr((long)0x100000000ULL, 9, 3), 3);
VALUE((long)0x100000000ULL && (1 / 0), 0);
/* Pointer arithmetic on invented addresses is evaluator normalization coverage,
 * not a defined C ICE oracle. Likewise exclude the preserved out-of-range
 * literal fallback and general evaluator's wrapping/saturating extensions. */
