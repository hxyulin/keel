/* Emit an object that exercises storage, ABI signatures, forward calls and callbacks. */
#include "keel.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static keel_module *module;
static void check(keel_status status, const char *operation, int line) {
    if (status != KEEL_OK) {
        fprintf(stderr, "systems_client.c:%d: %s: %s\n", line, operation, keel_module_error(module));
        keel_module_destroy(module);
        exit(1);
    }
}
#define CHECK(operation) check((operation), #operation, __LINE__)

static keel_signature signature(const keel_abi_param *params, size_t count,
                                const keel_abi_param *returns, size_t return_count) {
    keel_signature result;
    CHECK(keel_signature_create(module, params, count, returns, return_count, &result));
    return result;
}

static keel_symbol declare_function(const char *name, keel_signature sig) {
    keel_symbol symbol;
    CHECK(keel_module_declare_function(module, (const uint8_t *)name, strlen(name), sig, &symbol));
    return symbol;
}

static keel_function start(const char *name, keel_signature sig, const keel_type *types,
                           size_t count, keel_block *entry, keel_value *params) {
    keel_function function;
    CHECK(keel_module_add_function_with_signature(module, (const uint8_t *)name, strlen(name), sig, &function));
    CHECK(keel_block_create(module, function, entry));
    for (size_t i = 0; i < count; ++i) CHECK(keel_block_add_parameter(module, *entry, types[i], &params[i]));
    return function;
}

static keel_value iconst(keel_block b, keel_type type, int64_t n) {
    keel_value value;
    CHECK(keel_build_iconst(module, b, type, n, &value));
    return value;
}

static keel_value add(keel_block b, keel_value lhs, keel_value rhs) {
    keel_value value;
    CHECK(keel_build_binary(module, b, KEEL_IADD, lhs, rhs, &value));
    return value;
}

static keel_value address(keel_block b, keel_symbol symbol) {
    keel_value value;
    CHECK(keel_build_symbol_address(module, b, symbol, &value));
    return value;
}

static keel_value load(keel_block b, keel_type type, keel_value addr, keel_memory memory) {
    keel_value value;
    CHECK(keel_build_load(module, b, type, addr, memory, &value));
    return value;
}

static keel_value convert(keel_block b, keel_convert_op op, keel_type type, keel_value source) {
    keel_value value;
    CHECK(keel_build_convert(module, b, op, type, source, &value));
    return value;
}

int main(int argc, char **argv) {
    if (argc != 4) return 1;
    keel_target target;
    if (strcmp(argv[1], "darwin-arm64") == 0) target = KEEL_AARCH64_APPLE_DARWIN;
    else if (strcmp(argv[1], "linux-arm64") == 0) target = KEEL_AARCH64_LINUX_GNU;
    else if (strcmp(argv[1], "linux-x86_64") == 0) target = KEEL_X86_64_LINUX_GNU;
    else return 1;
    module = keel_module_create();
    const keel_abi_param i64 = { KEEL_I64, KEEL_ABI_NONE };
    const keel_abi_param ptr = { KEEL_PTR, KEEL_ABI_NONE };
    const keel_abi_param i32 = { KEEL_I32, KEEL_ABI_NONE };
    const keel_abi_param i8 = { KEEL_I8, KEEL_ABI_SIGN_EXTEND };
    const keel_abi_param u16 = { KEEL_I16, KEEL_ABI_ZERO_EXTEND };
    const keel_type t64[] = { KEEL_I64 }, tptr[] = { KEEL_PTR };
    const keel_memory plain = { 1, 0 }, aligned = { 8, 0 }, vol = { 8, KEEL_MEMORY_VOLATILE };
    keel_signature unary64 = signature(&i64, 1, &i64, 1);
    keel_signature touch_sig = signature(&ptr, 1, NULL, 0);
    const keel_abi_param adjust_params[] = { i8, u16 };
    keel_signature adjust_sig = signature(adjust_params, 2, &i64, 1);
    keel_symbol twice = declare_function("twice", unary64);
    keel_symbol host_touch = declare_function("host_touch", touch_sig);
    keel_symbol host_adjust = declare_function("host_adjust", adjust_sig);
    keel_symbol host_byte = declare_function("host_byte", signature(NULL, 0, &i8, 1));

    keel_symbol counter, bias, bias_pointer, external, seed, helper_pointer;
    CHECK(keel_module_declare_global(module, (const uint8_t *)"counter", 7, &counter));
    CHECK(keel_module_add_zero_global(module, (const uint8_t *)"counter", 7, 8, 8, 0, &counter));
    const uint8_t bias_bytes[] = { 5,0,0,0,0,0,0,0, 9,0,0,0,0,0,0,0 };
    CHECK(keel_module_add_global(module, (const uint8_t *)"bias", 4, bias_bytes, sizeof(bias_bytes), 8,
                               KEEL_GLOBAL_CONSTANT | KEEL_GLOBAL_LOCAL, &bias));
    CHECK(keel_module_add_pointer_global(module, (const uint8_t *)"bias_pointer", 12, bias, 8,
                                       KEEL_GLOBAL_CONSTANT | KEEL_GLOBAL_LOCAL, &bias_pointer));
    CHECK(keel_module_declare_global(module, (const uint8_t *)"external_value", 14, &external));
    const uint8_t seed_bytes[] = { 7,0,0,0,0,0,0,0 };
    CHECK(keel_module_add_global(module, (const uint8_t *)"seed", 4, seed_bytes, 8, 8, 0, &seed));
    CHECK(keel_module_add_pointer_global(module, (const uint8_t *)"helper_pointer", 14, twice, 0,
                                       KEEL_GLOBAL_CONSTANT, &helper_pointer));

    const keel_abi_param run_params[] = { ptr, ptr, i64 };
    const keel_type run_types[] = { KEEL_PTR, KEEL_PTR, KEEL_I64 };
    keel_signature run_sig = signature(run_params, 3, &i64, 1);
    keel_value args[3];
    keel_block b;
    keel_function f = start("system_run", run_sig, run_types, 3, &b, args);
    keel_value a = load(b, KEEL_I64, args[0], vol);
    keel_value second, offset = iconst(b, KEEL_I64, 8);
    CHECK(keel_build_ptradd(module, b, args[0], offset, &second));
    keel_value sum = add(b, a, load(b, KEEL_I64, second, plain));
    CHECK(keel_build_store(module, b, sum, second, aligned));
    keel_stack_slot slot;
    CHECK(keel_stack_slot_create(module, f, 8, 8, &slot));
    keel_value scratch;
    CHECK(keel_build_stack_address(module, b, slot, &scratch));
    CHECK(keel_build_store(module, b, args[2], scratch, aligned));
    CHECK(keel_build_call(module, b, host_touch, &scratch, 1, NULL, 0));
    keel_value touched = load(b, KEEL_I64, scratch, aligned), doubled, callback_result;
    CHECK(keel_build_call(module, b, twice, &touched, 1, &doubled, 1));
    CHECK(keel_build_call_indirect(module, b, unary64, args[1], &doubled, 1, &callback_result, 1));
    keel_value narrow_args[] = { convert(b, KEEL_TRUNC, KEEL_I8, a), iconst(b, KEEL_I16, 65530) };
    keel_value adjusted;
    CHECK(keel_build_call(module, b, host_adjust, narrow_args, 2, &adjusted, 1));
    keel_value bias_addr = load(b, KEEL_PTR, address(b, bias_pointer), aligned);
    keel_value bias_value = load(b, KEEL_I64, bias_addr, aligned);
    keel_value external_value = load(b, KEEL_I64, address(b, external), aligned);
    keel_value seed_value = load(b, KEEL_I64, address(b, seed), aligned);
    keel_value counter_addr = address(b, counter), old_count = load(b, KEEL_I64, counter_addr, vol);
    keel_value next_count = add(b, old_count, iconst(b, KEEL_I64, 1));
    CHECK(keel_build_store(module, b, next_count, counter_addr, vol));
    keel_value result = add(b, callback_result, adjusted);
    result = add(b, result, sum);
    result = add(b, result, bias_value);
    result = add(b, result, external_value);
    result = add(b, result, seed_value);
    result = add(b, result, old_count);
    CHECK(keel_build_return(module, b, &result, 1));

    /* Complete a declaration after calls and a function-pointer initializer use it. */
    CHECK(keel_module_define_function(module, twice, &f));
    CHECK(keel_function_set_linkage(module, f, KEEL_LOCAL));
    CHECK(keel_block_create(module, f, &b));
    CHECK(keel_block_add_parameter(module, b, KEEL_I64, &a));
    result = add(b, a, a);
    CHECK(keel_build_return(module, b, &result, 1));
    keel_symbol same_symbol;
    CHECK(keel_function_symbol(module, f, &same_symbol));
    if (same_symbol.owner != twice.owner || same_symbol.index != twice.index) return 1;

    start("use_pointer_global", unary64, t64, 1, &b, args);
    keel_value helper = load(b, KEEL_PTR, address(b, helper_pointer), aligned);
    CHECK(keel_build_call_indirect(module, b, unary64, helper, args, 1, &result, 1));
    CHECK(keel_build_return(module, b, &result, 1));

    /* Taking a function address produces a callable pointer, not a host address. */
    start("use_function_address", unary64, t64, 1, &b, args);
    helper = address(b, twice);
    CHECK(keel_build_call_indirect(module, b, unary64, helper, args, 1, &result, 1));
    CHECK(keel_build_return(module, b, &result, 1));

    start("roundtrip", signature(&ptr, 1, &ptr, 1), tptr, 1, &b, args);
    result = convert(b, KEEL_INT_TO_PTR, KEEL_PTR, convert(b, KEEL_PTR_TO_INT, KEEL_I64, args[0]));
    CHECK(keel_build_return(module, b, &result, 1));
    start("is_null", signature(&ptr, 1, &i32, 1), tptr, 1, &b, args);
    keel_value null_value = convert(b, KEEL_INT_TO_PTR, KEEL_PTR, iconst(b, KEEL_I64, 0));
    CHECK(keel_build_icmp(module, b, KEEL_EQ, args[0], null_value, &result));
    result = convert(b, KEEL_UEXT, KEEL_I32, result);
    CHECK(keel_build_return(module, b, &result, 1));

    const keel_type t8[] = { KEEL_I8 }, t16[] = { KEEL_I16 };
    start("echo_signed", signature(&i8, 1, &i8, 1), t8, 1, &b, args);
    CHECK(keel_build_return(module, b, args, 1));
    start("echo_unsigned", signature(&u16, 1, &u16, 1), t16, 1, &b, args);
    CHECK(keel_build_return(module, b, args, 1));
    start("call_host_byte", signature(NULL, 0, &i64, 1), NULL, 0, &b, NULL);
    CHECK(keel_build_call(module, b, host_byte, NULL, 0, &result, 1));
    result = convert(b, KEEL_SEXT, KEEL_I64, result);
    CHECK(keel_build_return(module, b, &result, 1));

    keel_abi_param many_params[10];
    for (size_t i = 0; i < 10; ++i) many_params[i] = i64;
    keel_symbol host_many = declare_function("host_many", signature(many_params, 10, &i64, 1));
    start("many_arguments", signature(NULL, 0, &i64, 1), NULL, 0, &b, NULL);
    keel_value many_args[10];
    for (size_t i = 0; i < 10; ++i) many_args[i] = iconst(b, KEEL_I64, (int64_t)i + 1);
    CHECK(keel_build_call(module, b, host_many, many_args, 10, &result, 1));
    CHECK(keel_build_return(module, b, &result, 1));

    /* Loop-carried SSA values, dynamic pointer offsets and exit block arguments. */
    const keel_abi_param loop_params[] = { ptr, i64 };
    const keel_type loop_types[] = { KEEL_PTR, KEEL_I64 };
    f = start("sum_loop", signature(loop_params, 2, &i64, 1), loop_types, 2, &b, args);
    keel_block header, body, exit_block;
    CHECK(keel_block_create(module, f, &header));
    CHECK(keel_block_create(module, f, &body));
    CHECK(keel_block_create(module, f, &exit_block));
    keel_value index, accumulator, exit_result;
    CHECK(keel_block_add_parameter(module, header, KEEL_I64, &index));
    CHECK(keel_block_add_parameter(module, header, KEEL_I64, &accumulator));
    CHECK(keel_block_add_parameter(module, exit_block, KEEL_I64, &exit_result));
    keel_value zero = iconst(b, KEEL_I64, 0), initial[] = { zero, zero };
    CHECK(keel_build_jump(module, b, header, initial, 2));
    keel_value condition;
    CHECK(keel_build_icmp(module, header, KEEL_SLT, index, args[1], &condition));
    CHECK(keel_build_brif(module, header, condition, body, NULL, 0, exit_block, &accumulator, 1));
    keel_value stride = iconst(body, KEEL_I64, 8), byte_offset, element;
    CHECK(keel_build_binary(module, body, KEEL_IMUL, index, stride, &byte_offset));
    CHECK(keel_build_ptradd(module, body, args[0], byte_offset, &element));
    keel_value next_sum = add(body, accumulator, load(body, KEEL_I64, element, aligned));
    keel_value next_index = add(body, index, iconst(body, KEEL_I64, 1));
    keel_value backedge[] = { next_index, next_sum };
    CHECK(keel_build_jump(module, body, header, backedge, 2));
    CHECK(keel_build_return(module, exit_block, &exit_result, 1));

    CHECK(keel_module_verify(module));
    if (strcmp(argv[3], "optimized") == 0) CHECK(keel_module_optimize(module));
    else if (strcmp(argv[3], "baseline") != 0) return 1;
    keel_buffer *object;
    CHECK(keel_module_emit_object(module, target, &object));
    keel_module_destroy(module);
    FILE *file = fopen(argv[2], "wb");
    if (!file) { keel_buffer_destroy(object); return 1; }
    size_t size = keel_buffer_size(object);
    int written = fwrite(keel_buffer_data(object), 1, size, file) == size;
    int closed = fclose(file) == 0;
    keel_buffer_destroy(object);
    return written && closed ? 0 : 1;
}
