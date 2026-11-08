/* Builds int64_t max_plus_seven(int64_t a, int64_t b), then writes an object. */
#include "keel.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static keel_module *module;
static void check(keel_status status) {
    if (status != KEEL_OK) {
        fprintf(stderr, "keel: %s\n", keel_module_error(module));
        keel_module_destroy(module);
        exit(1);
    }
}

int main(int argc, char **argv) {
    if (argc != 3) {
        fprintf(stderr, "usage: %s darwin-arm64|linux-arm64|linux-x86_64 output.o\n", argv[0]);
        return 1;
    }
    keel_target target;
    if (strcmp(argv[1], "darwin-arm64") == 0) target = KEEL_AARCH64_APPLE_DARWIN;
    else if (strcmp(argv[1], "linux-arm64") == 0) target = KEEL_AARCH64_LINUX_GNU;
    else if (strcmp(argv[1], "linux-x86_64") == 0) target = KEEL_X86_64_LINUX_GNU;
    else return 1;
    if (keel_abi_version() != 1) return 1;

    module = keel_module_create();
    const keel_type params[] = { KEEL_I64, KEEL_I64 };
    const keel_type returns[] = { KEEL_I64 };
    const char name[] = "max_plus_seven";
    keel_function function;
    check(keel_module_add_function(module, (const uint8_t *)name, sizeof(name) - 1,
                                  params, 2, returns, 1, &function));
    keel_block entry, left, right, merge;
    check(keel_block_create(module, function, &entry));
    check(keel_block_create(module, function, &left));
    check(keel_block_create(module, function, &right));
    check(keel_block_create(module, function, &merge));
    keel_value a, b, selected, condition, seven, result;
    check(keel_block_add_parameter(module, entry, KEEL_I64, &a));
    check(keel_block_add_parameter(module, entry, KEEL_I64, &b));
    check(keel_block_add_parameter(module, merge, KEEL_I64, &selected));
    check(keel_build_icmp(module, entry, KEEL_SGT, a, b, &condition));
    check(keel_build_brif(module, entry, condition, left, NULL, 0, right, NULL, 0));
    check(keel_build_jump(module, left, merge, &a, 1));
    check(keel_build_jump(module, right, merge, &b, 1));
    check(keel_build_iconst(module, merge, KEEL_I64, 7, &seven));
    check(keel_build_binary(module, merge, KEEL_IADD, selected, seven, &result));
    check(keel_build_return(module, merge, &result, 1));
    check(keel_module_verify(module));
    /* Existing IDs must not be used after optimization. */
    check(keel_module_optimize(module));
    keel_buffer *object = NULL;
    check(keel_module_emit_object(module, target, &object));
    /* The output survives destruction of the module. */
    keel_module_destroy(module);
    FILE *file = fopen(argv[2], "wb");
    if (!file) { keel_buffer_destroy(object); return 1; }
    const size_t size = keel_buffer_size(object);
    const int written = fwrite(keel_buffer_data(object), 1, size, file) == size;
    const int closed = fclose(file) == 0;
    keel_buffer_destroy(object);
    return written && closed ? 0 : 1;
}
