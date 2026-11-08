#include "keel.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static keel_string text(const char *s) {
    keel_string result = {(const uint8_t *)s, strlen(s)};
    return result;
}
static uint8_t *read_bytes(const char *path, size_t *size) {
    FILE *file = fopen(path, "rb");
    if (!file || fseek(file, 0, SEEK_END) != 0) exit(2);
    long length = ftell(file);
    if (length < 0 || fseek(file, 0, SEEK_SET) != 0) exit(2);
    *size = (size_t)length;
    uint8_t *bytes = (uint8_t *)malloc(*size);
    if (!bytes || fread(bytes, 1, *size, file) != *size) exit(2);
    fclose(file);
    return bytes;
}
int main(int argc, char **argv) {
    if (argc != 5 || keel_abi_version() != 1) return 2;
    keel_link_input inputs[2];
    inputs[0].name = text("entry.o");
    inputs[0].data = read_bytes(argv[1], &inputs[0].length);
    inputs[1].name = text("helpers.a");
    inputs[1].data = read_bytes(argv[2], &inputs[1].length);
    char tbd_path[4096];
    int count = snprintf(tbd_path, sizeof(tbd_path), "%s/usr/lib/libSystem.tbd", argv[3]);
    if (count < 0 || (size_t)count >= sizeof(tbd_path)) return 2;
    keel_string tbd = text(tbd_path);
    keel_link_options options = {text(argv[3]), text(argv[4]), KEEL_LINK_DEAD_STRIP};
    keel_buffer *warnings = NULL, *error = NULL;
    keel_status status = keel_link_signed(inputs, 2, &tbd, 1, &options, &warnings, &error);
    free((void *)inputs[0].data);
    free((void *)inputs[1].data);
    if (status != KEEL_OK) {
        fwrite(keel_buffer_data(error), 1, keel_buffer_size(error), stderr);
        keel_buffer_destroy(error);
        return 1;
    }
    if (error != NULL || warnings == NULL) return 3;
    keel_buffer_destroy(warnings);
    return 0;
}
