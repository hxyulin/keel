/* Link caller-supplied Linux startup/object/library bytes without a host linker. */
#include "keel.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

int main(int argc, char **argv) {
    if (argc < 4 || keel_abi_version() != 1) return 2;
    keel_elf_link_options options = {0};
    if (strcmp(argv[1], "linux-arm64") == 0) options.target = KEEL_AARCH64_LINUX_GNU;
    else if (strcmp(argv[1], "linux-x86_64") == 0) options.target = KEEL_X86_64_LINUX_GNU;
    else return 2;
    size_t count = (size_t)argc - 3;
    keel_link_input *inputs = (keel_link_input *)calloc(count, sizeof(*inputs));
    if (!inputs) return 2;
    for (size_t i = 0; i < count; ++i) {
        const char *path = argv[i + 3];
        FILE *file = fopen(path, "rb");
        if (!file || fseek(file, 0, SEEK_END) != 0) return 2;
        long size = ftell(file);
        if (size < 0 || fseek(file, 0, SEEK_SET) != 0) return 2;
        uint8_t *bytes = (uint8_t *)malloc((size_t)size + 1);
        if (!bytes || fread(bytes, 1, (size_t)size, file) != (size_t)size) return 2;
        fclose(file);
        inputs[i].name.data = (const uint8_t *)path;
        inputs[i].name.length = strlen(path);
        inputs[i].data = bytes;
        inputs[i].length = (size_t)size;
    }
    keel_buffer *image = NULL, *warnings = NULL, *error = NULL;
    keel_status status = keel_link_elf(inputs, count, &options, &image, &warnings, &error);
    for (size_t i = 0; i < count; ++i) free((void *)inputs[i].data);
    free(inputs);
    if (status != KEEL_OK) {
        if (error) fwrite(keel_buffer_data(error), 1, keel_buffer_size(error), stderr);
        keel_buffer_destroy(error);
        return 1;
    }
    fwrite(keel_buffer_data(warnings), 1, keel_buffer_size(warnings), stderr);
    keel_buffer_destroy(warnings);
    FILE *file = fopen(argv[2], "wb");
    if (!file) { keel_buffer_destroy(image); return 1; }
    size_t size = keel_buffer_size(image);
    int written = fwrite(keel_buffer_data(image), 1, size, file) == size;
    int closed = fclose(file) == 0;
    keel_buffer_destroy(image);
    return written && closed ? 0 : 1;
}
