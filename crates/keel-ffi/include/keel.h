#ifndef KEEL_H
#define KEEL_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Experimental ABI revision 1. Check keel_abi_version() before using this header.
 *
 * Ownership and safety:
 * - Modules own all IR, symbols, signatures and stack slots. IDs are values,
 *   not pointers, and need no destruction.
 *   IDs are checked for module, function, bounds and optimization generation.
 *   Do not fabricate or modify IDs. Creating functions/blocks preserves IDs.
 * - Serialize access to each module, including diagnostics and destruction.
 *   Distinct modules can be used concurrently. Buffers can be read concurrently.
 * - All nonnull pointers must be live and properly aligned. Input arrays must
 *   contain the stated number of initialized elements in one readable allocation.
 *   Inputs are borrowed only for the call. Null input is accepted for length 0.
 * - Output pointers must be writable and must not overlap inputs or API storage.
 *   Outputs are written only on success. Do not overwrite an owned buffer pointer
 *   without first destroying its previous buffer.
 * - Null module/output pointers return KEEL_INVALID_ARGUMENT. Arbitrary dangling
 *   or otherwise invalid pointers are caller errors, not recoverable statuses.
 * - Every status-returning call on a live, usable module replaces its diagnostic.
 *   Success clears it. Copy diagnostics before the next operation on that module.
 * - An unexpected unwinding Rust panic returns KEEL_INTERNAL_PANIC and poisons
 *   the module. Further operations return KEEL_POISONED and keep the diagnostic.
 *   Only error inspection and destruction remain available. Builds using Rust's
 *   panic=abort, allocation failure and process faults can terminate the process.
 * - Buffers own bytes independently of modules. Only keel_buffer_destroy frees
 *   them. Buffer bytes are immutable and are NOT NUL-terminated.
 * - This API builds scalar/vector SSA functions with zero or one return and
 *   a C or explicit private calling convention. Narrow ABI extension is explicit in
 *   keel_abi_param. Arithmetic follows Keel IR semantics, not C promotion.
 * - Pointers are values in the GENERATED program. Load/store/call builders
 *   never dereference or execute them in the compiler process. Their runtime
 *   validity and alignment are the frontend's responsibility. A checked ID
 *   does not prove a generated memory access is valid.
 * - All currently exposed object targets use 64-bit little-endian pointers.
 *   Object emission does not link or load code.
 * - Target-specific acceptance is checked by the core verifier and backend.
 *   Exposing a type/intrinsic does not imply every target can lower it.
 *   Complete textual IR input covers operations/metadata without typed wrappers.
 *   Object emission does not perform JIT allocation or loading.
 */

typedef struct KeelModule keel_module;
typedef struct KeelBuffer keel_buffer;
typedef struct { uint64_t owner; uint32_t index; } keel_function;
typedef struct { uint64_t owner; uint32_t index; } keel_signature;
typedef struct { uint64_t owner; uint32_t index; } keel_symbol;
typedef struct { uint64_t owner; uint32_t function; uint32_t index; } keel_stack_slot;
typedef struct { uint64_t owner; uint32_t function; uint32_t index; } keel_block;
typedef struct { uint64_t owner; uint32_t function; uint32_t index; } keel_value;

/* Fixed-width tags, rather than C enums in signatures. Unknown tags are rejected. */
typedef uint32_t keel_status;
enum {
    KEEL_OK = 0,
    KEEL_INVALID_ARGUMENT = 1,
    KEEL_VERIFICATION_FAILED = 2,
    KEEL_COMPILATION_FAILED = 3,
    KEEL_INTERNAL_PANIC = 4,
    KEEL_POISONED = 5
};
typedef uint32_t keel_type;
enum { KEEL_I8 = 1, KEEL_I16 = 2, KEEL_I32 = 3, KEEL_I64 = 4, KEEL_PTR = 5, KEEL_I128 = 6, KEEL_F32 = 7, KEEL_F64 = 8, KEEL_F80 = 9 };
typedef uint32_t keel_binary_op;
enum {
    KEEL_IADD = 1, KEEL_ISUB = 2, KEEL_IMUL = 3,
    KEEL_UDIV = 4, KEEL_SDIV = 5, KEEL_UREM = 6, KEEL_SREM = 7,
    KEEL_AND = 8, KEEL_OR = 9, KEEL_XOR = 10,
    KEEL_SHL = 11, KEEL_USHR = 12, KEEL_SSHR = 13,
    KEEL_ROTL = 14, KEEL_ROTR = 15, KEEL_UMULHI = 16, KEEL_SMULHI = 17,
    KEEL_FADD = 20, KEEL_FSUB = 21, KEEL_FMUL = 22, KEEL_FDIV = 23,
    KEEL_FMIN = 24, KEEL_FMAX = 25, KEEL_FCOPYSIGN = 26
};
typedef uint32_t keel_int_cc;
enum {
    KEEL_EQ = 1, KEEL_NE = 2, KEEL_SLT = 3, KEEL_SLE = 4,
    KEEL_SGT = 5, KEEL_SGE = 6, KEEL_ULT = 7, KEEL_ULE = 8,
    KEEL_UGT = 9, KEEL_UGE = 10
};
typedef uint32_t keel_target;
enum {
    KEEL_AARCH64_APPLE_DARWIN = 1,
    KEEL_AARCH64_LINUX_GNU = 2,
    KEEL_X86_64_LINUX_GNU = 3
};

typedef uint32_t keel_abi_extension;
enum { KEEL_ABI_NONE = 0, KEEL_ABI_SIGN_EXTEND = 1, KEEL_ABI_ZERO_EXTEND = 2 };
typedef struct { keel_type type; keel_abi_extension extension; } keel_abi_param;
/* i8/i16 require SIGN_EXTEND or ZERO_EXTEND. All other types require NONE.
 * Signatures are immutable and copied into definitions, declarations and calls.
 * Use keel_signature_create_full for aggregate, variadic or private signatures. */

typedef uint32_t keel_convert_op;
enum {
    KEEL_SEXT = 1, KEEL_UEXT = 2, KEEL_TRUNC = 3,
    KEEL_PTR_TO_INT = 4, KEEL_INT_TO_PTR = 5,
    KEEL_FPEXT = 6, KEEL_FPTRUNC = 7, KEEL_FPTOSI = 8, KEEL_FPTOUI = 9,
    KEEL_SITOFP = 10, KEEL_UITOFP = 11, KEEL_BITCAST = 12
};
typedef uint32_t keel_memory_flags;
enum { KEEL_MEMORY_VOLATILE = 1, KEEL_MEMORY_NOTRAP = 2, KEEL_MEMORY_TRAP_UB = 4 };
typedef struct { uint32_t align; keel_memory_flags flags; } keel_memory;
/* {1, 0} is conservative memory access. align is a nonzero power of two.
 * VOLATILE preserves accesses; it is neither atomic nor a thread fence.
 * NOTRAP promises that this access cannot fault. TRAP_UB makes faults undefined
 * behavior and cannot combine with VOLATILE. Do not assert promises your source
 * language does not provide. These flags affect optimization of generated code. */

typedef uint32_t keel_linkage;
enum { KEEL_EXPORTED = 0, KEEL_LOCAL = 1 };
typedef uint32_t keel_global_flags;
enum { KEEL_GLOBAL_CONSTANT = 1, KEEL_GLOBAL_LOCAL = 2 };

uint32_t keel_abi_version(void);
keel_module *keel_module_create(void);
void keel_module_destroy(keel_module *module); /* null is a no-op */
const char *keel_module_error(const keel_module *module); /* null -> null */

/* Names are copied UTF-8 bytes, without a trailing NUL. Names must start with
 * an ASCII letter, '_', '.' or '$', followed by those characters or digits.
 * Entry block parameters must be added explicitly in signature order.
 * This convenience constructor accepts all exposed types except i8/i16. For i8/i16, use an explicit
 * signature with the required ABI extension. Names share one module namespace. */
keel_status keel_module_add_function(
    keel_module *module, const uint8_t *name, size_t name_len,
    const keel_type *params, size_t param_count,
    const keel_type *returns, size_t return_count, keel_function *out);
/* The first block created is the entry. Blocks have explicit SSA parameters. */
keel_status keel_block_create(keel_module *module, keel_function function, keel_block *out);
keel_status keel_block_add_parameter(
    keel_module *module, keel_block block, keel_type type, keel_value *out);
/* Append instructions before a terminator. Add block parameters before its
 * first instruction. Dominance and complete CFG checks happen in verify. */
keel_status keel_build_iconst(
    keel_module *module, keel_block block, keel_type type, int64_t immediate, keel_value *out);
keel_status keel_build_binary(
    keel_module *module, keel_block block, keel_binary_op op,
    keel_value lhs, keel_value rhs, keel_value *out);
keel_status keel_build_icmp(
    keel_module *module, keel_block block, keel_int_cc condition,
    keel_value lhs, keel_value rhs, keel_value *out);
keel_status keel_build_jump(
    keel_module *module, keel_block block, keel_block dest, const keel_value *args, size_t count);
/* condition must be i8. Zero takes else; nonzero takes then. */
keel_status keel_build_brif(
    keel_module *module, keel_block block, keel_value condition,
    keel_block then_block, const keel_value *then_args, size_t then_count,
    keel_block else_block, const keel_value *else_args, size_t else_count);
keel_status keel_build_return(
    keel_module *module, keel_block block, const keel_value *args, size_t count);

keel_status keel_module_verify(keel_module *module);
/* Verifies first. Once optimization starts, ALL prior function/block/value IDs
 * (including signatures, symbols and stack slots) become stale, even if
 * optimization fails. Emit IR/objects after optimization;
 * find_function/find_symbol and block lookup APIs return fresh IDs after rewriting.
 * Verification failure before optimization starts preserves IDs. Uses keel_opt::optimize. */
keel_status keel_module_optimize(keel_module *module);
/* IR text can be inspected even while a module is under construction. */
keel_status keel_module_emit_ir(keel_module *module, keel_buffer **out);
/* Always verifies first. Returns Mach-O for Darwin, ELF for Linux. */
keel_status keel_module_emit_object(keel_module *module, keel_target target, keel_buffer **out);

/* Scalar ABI signatures and function symbols. All IDs follow the same module
 * ownership/generation rules as block and value IDs. A symbol can be shared by
 * calls in different functions; values and stack slots cannot. */
keel_status keel_signature_create(
    keel_module *module, const keel_abi_param *params, size_t param_count,
    const keel_abi_param *returns, size_t return_count, keel_signature *out);
keel_status keel_module_add_function_with_signature(
    keel_module *module, const uint8_t *name, size_t name_len,
    keel_signature signature, keel_function *out);
keel_status keel_module_declare_function(
    keel_module *module, const uint8_t *name, size_t name_len,
    keel_signature signature, keel_symbol *out);
/* A forward declaration can become a definition exactly once. Its symbol stays
 * valid, including for calls already built. Recursion and mutual recursion can
 * use forward symbols. Duplicate declarations/definitions are rejected. */
keel_status keel_module_define_function(
    keel_module *module, keel_symbol symbol, keel_function *out);
keel_status keel_function_symbol(
    keel_module *module, keel_function function, keel_symbol *out);
keel_status keel_function_set_linkage(
    keel_module *module, keel_function function, keel_linkage linkage);

/* Stack slots live until the GENERATED function returns; creating a slot does
 * not allocate memory in the host. Size is nonzero; align is a power of two up
 * to 64. No implicit initialization, bounds checks or lifetime extension. */
keel_status keel_stack_slot_create(
    keel_module *module, keel_function function, uint32_t size, uint32_t align,
    keel_stack_slot *out);
keel_status keel_build_stack_address(
    keel_module *module, keel_block block, keel_stack_slot slot, keel_value *out);
/* Offset is i64 bytes, without implicit element-size scaling or bounds checks. */
keel_status keel_build_ptradd(
    keel_module *module, keel_block block, keel_value address, keel_value offset, keel_value *out);
keel_status keel_build_load(
    keel_module *module, keel_block block, keel_type type, keel_value address,
    keel_memory memory, keel_value *out);
keel_status keel_build_store(
    keel_module *module, keel_block block, keel_value value, keel_value address, keel_memory memory);
/* SEXT/UEXT widen integers; TRUNC narrows them. PTR_TO_INT is ptr -> i64;
 * INT_TO_PTR is i64 -> ptr. A null pointer is INT_TO_PTR of an i64 zero constant.
 * icmp also accepts equal-type pointers; use unsigned conditions for address
 * ordering. Ptr/int casts follow the currently supported native 64-bit model.
 * FPTOSI/FPTOUI saturate and map NaN to zero; a frontend requiring checked
 * conversions must add its own checks. Float extensions/truncations and bitcasts
 * follow core IR rules and are checked for source/destination width. */
keel_status keel_build_convert(
    keel_module *module, keel_block block, keel_convert_op op, keel_type type,
    keel_value value, keel_value *out);
keel_status keel_build_symbol_address(
    keel_module *module, keel_block block, keel_symbol symbol, keel_value *out);

/* Fixed call arguments must exactly match parameter types. Variadic tails
 * accept C-promoted i32/i64/ptr/f64 values; use explicit ABI calls for aggregates. Result count must exactly
 * match the signature's 0 or 1 return values. Pass NULL,0 for void calls; for a
 * value-returning call provide one writable result. Results are written only
 * on success and cannot overlap arguments. Indirect callee must have ptr type;
 * its runtime address must match the supplied signature. No host call occurs. */
keel_status keel_build_call(
    keel_module *module, keel_block block, keel_symbol symbol,
    const keel_value *args, size_t count, keel_value *results, size_t result_count);
keel_status keel_build_call_indirect(
    keel_module *module, keel_block block, keel_signature signature, keel_value callee,
    const keel_value *args, size_t count, keel_value *results, size_t result_count);

/* Module globals. Byte inputs are copied verbatim; the frontend supplies target
 * byte order and any aggregate padding. Size must be nonzero and <= UINT32_MAX.
 * Align is a power of two up to 65536. Flags are CONSTANT and/or LOCAL.
 * CONSTANT storage must not be written at runtime. Zero storage may use BSS.
 * Declared external storage can later be defined by the add_*_global functions
 * using the same name; the original symbol remains valid. A second definition
 * is rejected. No implicit TLS, weak binding or target sections are selected. */
keel_status keel_module_declare_global(
    keel_module *module, const uint8_t *name, size_t name_len, keel_symbol *out);
keel_status keel_module_add_global(
    keel_module *module, const uint8_t *name, size_t name_len,
    const uint8_t *bytes, size_t size, uint32_t align, keel_global_flags flags, keel_symbol *out);
keel_status keel_module_add_zero_global(
    keel_module *module, const uint8_t *name, size_t name_len,
    uint32_t size, uint32_t align, keel_global_flags flags, keel_symbol *out);
/* One 8-byte, align-8 pointer initialized by a linker relocation. addend is a
 * nonnegative byte offset, not an element count. Target is a function or data
 * symbol. General relocation arrays/aggregate initializers are not exposed. */
keel_status keel_module_add_pointer_global(
    keel_module *module, const uint8_t *name, size_t name_len,
    keel_symbol target, uint32_t addend, keel_global_flags flags, keel_symbol *out);

/* Additional numeric types and instructions. F80 and wide vectors remain
 * target-dependent. Vector tags come from keel_type_vector, not Rust enum casts.
 * Scalar fconst takes IEEE bits, preserving NaN payloads and signed zero.
 * Overflow/CAS outputs are [wrapped-or-old-value, i8-flag]. Unknown tags fail. */
typedef uint32_t keel_unary_op;
enum {
    KEEL_INEG = 1, KEEL_BNOT = 2, KEEL_CLZ = 3, KEEL_CTZ = 4,
    KEEL_POPCNT = 5, KEEL_BSWAP = 6, KEEL_FNEG = 7, KEEL_FABS = 8,
    KEEL_FSQRT = 9, KEEL_FCEIL = 10, KEEL_FFLOOR = 11,
    KEEL_FTRUNC = 12, KEEL_FNEAREST = 13
};
typedef uint32_t keel_overflow_op;
enum { KEEL_UADD_OVERFLOW = 1, KEEL_SADD_OVERFLOW = 2, KEEL_USUB_OVERFLOW = 3,
       KEEL_SSUB_OVERFLOW = 4, KEEL_UMUL_OVERFLOW = 5, KEEL_SMUL_OVERFLOW = 6 };
typedef uint32_t keel_float_cc;
enum { KEEL_FEQ = 1, KEEL_FNE = 2, KEEL_FLT = 3, KEEL_FLE = 4,
       KEEL_FGT = 5, KEEL_FGE = 6, KEEL_FORD = 7, KEEL_FUNO = 8 };
keel_status keel_type_vector(keel_module *, keel_type lane, uint32_t lanes, keel_type *out);
keel_status keel_value_type(keel_module *, keel_value, keel_type *out);
keel_status keel_build_fconst(keel_module *, keel_block, keel_type, uint64_t bits, keel_value *out);
keel_status keel_build_f80const(keel_module *, keel_block, uint64_t significand, uint16_t sign_exponent, keel_value *out);
keel_status keel_build_unary(keel_module *, keel_block, keel_unary_op, keel_value, keel_value *out);
keel_status keel_build_overflow(keel_module *, keel_block, keel_overflow_op,
    keel_value lhs, keel_value rhs, keel_value *results, size_t result_count);
keel_status keel_build_fcmp(keel_module *, keel_block, keel_float_cc, keel_value lhs, keel_value rhs, keel_value *out);
keel_status keel_build_fma(keel_module *, keel_block, keel_value a, keel_value b, keel_value c, keel_value *out);
keel_status keel_build_select(keel_module *, keel_block, keel_value condition, keel_value yes, keel_value no, keel_value *out);
keel_status keel_build_vsplat(keel_module *, keel_block, keel_type, keel_value, keel_value *out);
keel_status keel_build_vextract(keel_module *, keel_block, keel_value, uint32_t lane, keel_value *out);
keel_status keel_build_vinsert(keel_module *, keel_block, keel_value vector, keel_value value, uint32_t lane, keel_value *out);
keel_status keel_build_vshuffle(keel_module *, keel_block, keel_value lhs, keel_value rhs,
    const uint8_t *lanes, size_t count, keel_value *out);
/* Vector binary names are Keel IR names such as viadd/vfadd/vsadd_sat.
 * Reduction tags: 0 add, 1 and, 2 or, 3 xor, 4 smin, 5 smax, 6 umin, 7 umax. */
keel_status keel_build_vector_binary(keel_module *, keel_block, const uint8_t *operation, size_t length,
    keel_value lhs, keel_value rhs, keel_value *out);
keel_status keel_build_vector_reduce(keel_module *, keel_block, uint32_t operation, keel_value, keel_value *out);
keel_status keel_build_trap(keel_module *, keel_block);

/* Layouts use natural C scalar/struct/array rules, including padding. No packed,
 * union or raised-alignment ABI layout is invented by these constructors.
 * SRET is valid only as the first parameter of a void function. BYVAL carries
 * the object address; the call reads/copies its bytes at the call instruction.
 * The frontend must snapshot it earlier if source evaluation rules require it.
 * ABI return entries are normal scalars; aggregate results use an SRET parameter.
 * Normal ABI entries ignore layout, so a zero-initialized layout field is fine. */
typedef struct { uint64_t owner; uint32_t index; } keel_layout;
enum { KEEL_ABI_NORMAL = 0, KEEL_ABI_BYVAL = 1, KEEL_ABI_SRET = 2 };
typedef struct { keel_type type; keel_abi_extension extension; uint32_t purpose; keel_layout layout; } keel_abi_value;
enum { KEEL_CC_C = 0, KEEL_CC_FAST = 1 };
enum { KEEL_SIGNATURE_VARIADIC = 1, KEEL_SIGNATURE_RETURNS_TWICE = 2 };
enum { KEEL_X86_VECTOR_ABI_DEFAULT = 0, KEEL_X86_VECTOR_ABI_GNU = 1, KEEL_X86_VECTOR_ABI_CLANG = 2 };
typedef struct { uint32_t convention; uint32_t flags; uint32_t x86_vector_abi; } keel_signature_options;
typedef struct { keel_value value; keel_abi_value abi; } keel_call_argument;
keel_status keel_layout_scalar(keel_module *, keel_type, keel_layout *out);
keel_status keel_layout_struct(keel_module *, const keel_layout *fields, size_t count, keel_layout *out);
keel_status keel_layout_array(keel_module *, keel_layout element, uint32_t count, keel_layout *out);
keel_status keel_layout_size(keel_module *, keel_layout, uint64_t *out);
keel_status keel_layout_align(keel_module *, keel_layout, uint64_t *out);
keel_status keel_layout_field_offset(keel_module *, keel_layout, size_t index, uint64_t *out);
keel_status keel_signature_create_full(keel_module *, const keel_abi_value *params, size_t count,
    const keel_abi_value *returns, size_t return_count, keel_signature_options options, keel_signature *out);
keel_status keel_build_call_abi(keel_module *, keel_block, keel_symbol,
    const keel_call_argument *args, size_t count, keel_value *results, size_t result_count);
keel_status keel_build_call_indirect_abi(keel_module *, keel_block, keel_signature, keel_value callee,
    const keel_call_argument *args, size_t count, keel_value *results, size_t result_count);
keel_status keel_build_va_arg(keel_module *, keel_block, keel_type, keel_value list, keel_value *out);

/* Atomic addresses must be naturally aligned in the generated program. These
 * are atomic instructions, not compiler-host synchronization. Integer widths
 * and pointer atomics are further checked by the selected target backend.
 * CAS failure may be stronger than success, as in the core IR/C++17 contract,
 * but may not release. Weak CAS can fail spuriously. */
enum { KEEL_RELAXED = 0, KEEL_ACQUIRE = 1, KEEL_RELEASE = 2, KEEL_ACQ_REL = 3, KEEL_SEQ_CST = 4 };
enum { KEEL_RMW_ADD = 1, KEEL_RMW_SUB = 2, KEEL_RMW_AND = 3, KEEL_RMW_OR = 4, KEEL_RMW_XOR = 5,
       KEEL_RMW_EXCHANGE = 6, KEEL_RMW_UMIN = 7, KEEL_RMW_UMAX = 8, KEEL_RMW_SMIN = 9, KEEL_RMW_SMAX = 10 };
enum { KEEL_CAS_STRONG = 0, KEEL_CAS_WEAK = 1 };
keel_status keel_build_atomic_load(keel_module *, keel_block, keel_type, uint32_t order, keel_value address, keel_value *out);
keel_status keel_build_atomic_store(keel_module *, keel_block, uint32_t order, keel_value value, keel_value address);
keel_status keel_build_atomic_rmw(keel_module *, keel_block, uint32_t operation, uint32_t order,
    keel_value address, keel_value value, keel_value *out);
keel_status keel_build_atomic_cas(keel_module *, keel_block, uint32_t success, uint32_t failure, uint32_t strength,
    keel_value address, keel_value expected, keel_value replacement, keel_value *results, size_t result_count);
keel_status keel_build_fence(keel_module *, keel_block, uint32_t order);

/* All registry intrinsics are addressed by their IR names, including stack,
 * memory and target-specific operations. Signature checks happen immediately;
 * immediate/range/target constraints are checked by module verification/codegen.
 * Inline assembly uses the same template/constraint grammar as Keel IR.
 * Every nested switch-case argument span follows the input pointer contract. */
keel_status keel_build_intrinsic(keel_module *, keel_block, const uint8_t *name, size_t length,
    const keel_value *args, size_t count, keel_value *results, size_t result_count);
keel_status keel_build_neon(keel_module *, keel_block, const uint8_t *name, size_t length, keel_type,
    const keel_value *args, size_t count, keel_value *results, size_t result_count);
keel_status keel_build_inline_asm(keel_module *, keel_block, const uint8_t *template_text, size_t template_length,
    const uint8_t *constraints, size_t constraint_length, const keel_type *types, size_t result_count,
    const keel_value *args, size_t count, keel_value *results);
typedef struct { int64_t value; keel_block block; const keel_value *args; size_t count; } keel_switch_case;
keel_status keel_build_switch(keel_module *, keel_block, keel_value value,
    keel_block default_block, const keel_value *default_args, size_t default_count,
    const keel_switch_case *cases, size_t count);
enum { KEEL_DATA_TLS = 1, KEEL_DATA_WEAK = 2, KEEL_DATA_HIDDEN = 4,
       KEEL_DATA_RETAIN = 8, KEEL_DATA_WEAK_ODR = 16 };
keel_status keel_global_set_properties(keel_module *, keel_symbol, uint32_t flags);
/* Ordered initializer items: bytes reads bytes/length; zero reads length;
 * integer reads type/value; pointer reads symbol/addend. Other fields are ignored.
 * Items have no implicit padding. Supply padding as bytes or zero items. */
enum { KEEL_DATA_ITEM_BYTES = 0, KEEL_DATA_ITEM_ZERO = 1, KEEL_DATA_ITEM_INTEGER = 2, KEEL_DATA_ITEM_POINTER = 3 };
typedef struct {
    uint32_t kind; keel_type type; int64_t value; const uint8_t *bytes;
    size_t length; keel_symbol symbol; uint32_t addend;
} keel_data_item;
keel_status keel_module_add_global_items(keel_module *, const uint8_t *name, size_t length,
    const keel_data_item *items, size_t count, uint32_t align, keel_global_flags flags, keel_symbol *out);

/* Function flags replace the listed booleans; existing features/contracts and
 * optimization metadata are preserved. align=0 means default. Attribute changes
 * update existing direct-call/address references. Naked/interrupt metadata can
 * be represented, but native codegen currently rejects those entry contracts. */
enum {
    KEEL_FUNCTION_WEAK = UINT64_C(1) << 0,
    KEEL_FUNCTION_CONSTRUCTOR = UINT64_C(1) << 1,
    KEEL_FUNCTION_DESTRUCTOR = UINT64_C(1) << 2,
    KEEL_FUNCTION_USED = UINT64_C(1) << 3,
    KEEL_FUNCTION_RETAIN = UINT64_C(1) << 4,
    KEEL_FUNCTION_NONINTERPOSABLE = UINT64_C(1) << 5,
    KEEL_FUNCTION_NO_SEMANTIC_INTERPOSITION = UINT64_C(1) << 6,
    KEEL_FUNCTION_INLINEHINT = UINT64_C(1) << 7,
    KEEL_FUNCTION_ALWAYSINLINE = UINT64_C(1) << 8,
    KEEL_FUNCTION_NOINLINE = UINT64_C(1) << 9,
    KEEL_FUNCTION_NORETURN = UINT64_C(1) << 10,
    KEEL_FUNCTION_NOUNWIND = UINT64_C(1) << 11,
    KEEL_FUNCTION_COLD = UINT64_C(1) << 12,
    KEEL_FUNCTION_HOT = UINT64_C(1) << 13,
    KEEL_FUNCTION_OPTNONE = UINT64_C(1) << 14,
    KEEL_FUNCTION_FLATTEN = UINT64_C(1) << 15,
    KEEL_FUNCTION_NO_INSTRUMENT_FUNCTION = UINT64_C(1) << 16,
    KEEL_FUNCTION_NO_PROFILE_INSTRUMENT_FUNCTION = UINT64_C(1) << 17,
    KEEL_FUNCTION_NAKED = UINT64_C(1) << 18,
    KEEL_FUNCTION_INTERRUPT = UINT64_C(1) << 19
};
keel_status keel_symbol_set_function_attributes(keel_module *, keel_symbol, uint64_t flags, uint32_t align);

/* Complete IR access: load accepts every instruction/type/metadata form supported
 * by the core parser, including forms without typed C builders. It verifies
 * before replacing the module. Failure preserves IR and IDs; success invalidates
 * ALL IDs (including layout/source IDs). merge follows core merge restrictions,
 * notably no debug-info merging. Lookup APIs return new IDs after any rewrite.
 * Internal Rust passes/arenas/trait implementations are not C object layouts. */
keel_status keel_module_load_ir(keel_module *, const uint8_t *text, size_t length);
keel_status keel_module_merge_ir(keel_module *, const uint8_t *text, size_t length);
keel_status keel_module_find_function(keel_module *, const uint8_t *name, size_t length, keel_function *out);
keel_status keel_module_find_symbol(keel_module *, const uint8_t *name, size_t length, keel_symbol *out);
keel_status keel_function_block_at(keel_module *, keel_function, size_t ordinal, keel_block *out);
keel_status keel_block_parameter_at(keel_module *, keel_block, size_t ordinal, keel_value *out);
typedef struct { uint64_t owner; uint32_t index; } keel_source_file;
keel_status keel_source_file_add(keel_module *, const uint8_t *path, size_t length, keel_source_file *out);
/* Lines/columns are 1-based; zero means unknown. Ordinal is in block instruction order. */
keel_status keel_instruction_set_source(keel_module *, keel_block, size_t ordinal, keel_source_file, uint32_t line, uint32_t column);
keel_status keel_function_set_target_features(keel_module *, keel_function, const uint8_t *features, size_t length);
/* Optimization tiers: 0 none, 1 fast, 2 basic, 3 balanced, 4 size, 5 full.
 * Pass lists use core names separated by commas. Invalid policies do not rewrite IR. */
keel_status keel_module_optimize_with_tier(keel_module *, uint32_t tier);
keel_status keel_module_run_passes(keel_module *, const uint8_t *passes, size_t length);
/* All-zero options select defaults. Selection: 0 covers,1 baseline,2 trees,3 full.
 * Allocation: 0 full,1 fast. Goal: 0 speed,1 size. Unwind: 0 all,1 required-only.
 * x86 model: 0 implicit,1 PIC,2 small,3 kernel. Explicit models require x86.
 * Flags: 1 function sections,2 data sections,4 separate stack slots.
 * Experimental allocator variants and custom Rust pipeline traits are not exposed. */
typedef struct { uint32_t selection, allocation, goal, unwind, x86_code_model, flags; } keel_codegen_options;
keel_status keel_module_emit_assembly(keel_module *, keel_target, keel_codegen_options, keel_buffer **out);
keel_status keel_module_emit_object_with_options(keel_module *, keel_target, keel_codegen_options, keel_buffer **out);

const uint8_t *keel_buffer_data(const keel_buffer *buffer); /* null -> null */
size_t keel_buffer_size(const keel_buffer *buffer); /* null -> 0 */
void keel_buffer_destroy(keel_buffer *buffer); /* null is a no-op */

#ifdef __cplusplus
}
#endif
#endif
