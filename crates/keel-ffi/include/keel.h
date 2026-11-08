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
 * - This API builds integer/pointer SSA functions with zero or one return and
 *   a fixed-arity C calling convention. Narrow ABI extension is explicit in
 *   keel_abi_param. Arithmetic follows Keel IR semantics, not C promotion.
 * - Pointers are values in the GENERATED program. Load/store/call builders
 *   never dereference or execute them in the compiler process. Their runtime
 *   validity and alignment are the frontend's responsibility. A checked ID
 *   does not prove a generated memory access is valid.
 * - All currently exposed object targets use 64-bit little-endian pointers.
 *   Object emission does not link or load code.
 * - Floats, aggregate call ABIs, varargs, atomics, TLS, SIMD, debug info and JIT
 *   are not yet exposed.
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
enum { KEEL_I8 = 1, KEEL_I16 = 2, KEEL_I32 = 3, KEEL_I64 = 4, KEEL_PTR = 5 };
typedef uint32_t keel_binary_op;
enum {
    KEEL_IADD = 1, KEEL_ISUB = 2, KEEL_IMUL = 3,
    KEEL_UDIV = 4, KEEL_SDIV = 5, KEEL_UREM = 6, KEEL_SREM = 7,
    KEEL_AND = 8, KEEL_OR = 9, KEEL_XOR = 10,
    KEEL_SHL = 11, KEEL_USHR = 12, KEEL_SSHR = 13
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
 * No aggregate, variadic or private calling convention is exposed here. */

typedef uint32_t keel_convert_op;
enum {
    KEEL_SEXT = 1, KEEL_UEXT = 2, KEEL_TRUNC = 3,
    KEEL_PTR_TO_INT = 4, KEEL_INT_TO_PTR = 5
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
 * This convenience constructor accepts i32/i64/ptr. For i8/i16, use an explicit
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
 * there is currently no API to retrieve IDs for the rewritten IR. Verification
 * failure before optimization starts preserves IDs. Uses keel_opt::optimize. */
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
 * ordering. Ptr/int casts follow the currently supported native 64-bit model. */
keel_status keel_build_convert(
    keel_module *module, keel_block block, keel_convert_op op, keel_type type,
    keel_value value, keel_value *out);
keel_status keel_build_symbol_address(
    keel_module *module, keel_block block, keel_symbol symbol, keel_value *out);

/* Call arguments must exactly match parameter types. Result count must exactly
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

const uint8_t *keel_buffer_data(const keel_buffer *buffer); /* null -> null */
size_t keel_buffer_size(const keel_buffer *buffer); /* null -> 0 */
void keel_buffer_destroy(keel_buffer *buffer); /* null is a no-op */

#ifdef __cplusplus
}
#endif
#endif
