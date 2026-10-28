/* Untouched upstream library; this driver exercises the public DOM API. */
#include "yyjson.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>

typedef struct {
    uint64_t hash;
    uint64_t nodes;
    uint64_t string_bytes;
} digest;

static void byte(digest *d, unsigned char b) {
    d->hash = (d->hash ^ b) * UINT64_C(1099511628211);
}

static void word(digest *d, uint64_t n) {
    unsigned i;
    for (i = 0; i < 8; i++) { byte(d, (unsigned char)n); n >>= 8; }
}

static void string(digest *d, const char *s, size_t len) {
    size_t i;
    byte(d, 's'); word(d, len);
    for (i = 0; i < len; i++) byte(d, (unsigned char)s[i]);
    d->string_bytes += len;
}

static int visit(digest *d, yyjson_val *v, unsigned depth) {
    size_t i, count;
    yyjson_val *key, *value;
    if (!v || depth > 128) return 0;
    d->nodes++;
    if (yyjson_is_null(v)) byte(d, 'n');
    else if (yyjson_is_bool(v)) byte(d, yyjson_get_bool(v) ? 't' : 'f');
    else if (yyjson_is_str(v)) string(d, yyjson_get_str(v), yyjson_get_len(v));
    else if (yyjson_is_int(v)) {
        if (yyjson_is_sint(v) && yyjson_get_sint(v) < 0) {
            byte(d, '-'); word(d, UINT64_C(0) - (uint64_t)yyjson_get_sint(v));
        } else { byte(d, '+'); word(d, yyjson_get_uint(v)); }
    } else if (yyjson_is_real(v)) {
        double n = yyjson_get_real(v);
        uint64_t bits;
        memcpy(&bits, &n, sizeof(bits));
        byte(d, 'r'); word(d, bits);
    } else if (yyjson_is_arr(v)) {
        byte(d, '['); word(d, yyjson_arr_size(v));
        yyjson_arr_foreach(v, i, count, value) {
            if (!visit(d, value, depth + 1)) return 0;
        }
        byte(d, ']');
    } else if (yyjson_is_obj(v)) {
        byte(d, '{'); word(d, yyjson_obj_size(v));
        yyjson_obj_foreach(v, i, count, key, value) {
            string(d, yyjson_get_str(key), yyjson_get_len(key));
            if (!visit(d, value, depth + 1)) return 0;
        }
        byte(d, '}');
    } else return 0;
    return 1;
}

static digest initial(void) {
    digest d = {UINT64_C(14695981039346656037), 0, 0};
    return d;
}

static int same(digest a, digest b) {
    return a.hash == b.hash && a.nodes == b.nodes && a.string_bytes == b.string_bytes;
}

int main(int argc, char **argv) {
    FILE *file;
    long length, rounds, round;
    char *input, *end;
    digest expected = initial();
    if (argc != 3) { fprintf(stderr, "usage: yyjson-workload INPUT ROUNDS\n"); return 2; }
    rounds = strtol(argv[2], &end, 10);
    if (*end || rounds < 1 || rounds > 100000) return 2;
    file = fopen(argv[1], "rb");
    if (!file) return 2;
    if (fseek(file, 0, SEEK_END) || (length = ftell(file)) <= 0 ||
        length > 16777216 || fseek(file, 0, SEEK_SET)) { fclose(file); return 2; }
    input = (char *)malloc((size_t)length);
    if (!input) { fclose(file); return 2; }
    if (fread(input, 1, (size_t)length, file) != (size_t)length) {
        fclose(file); free(input); return 2;
    }
    fclose(file);
    for (round = 0; round < rounds; round++) {
        yyjson_read_err error;
        yyjson_doc *doc = yyjson_read_opts(input, (size_t)length, 0, NULL, &error);
        yyjson_doc *copy;
        digest before = initial(), after = initial();
        size_t written;
        char *output;
        if (!doc) {
            fprintf(stderr, "parse error at %zu: %s\n", error.pos, error.msg);
            free(input); return 3; /* Distinct parser rejection, not an internal failure. */
        }
        if (!visit(&before, yyjson_doc_get_root(doc), 0)) {
            yyjson_doc_free(doc); free(input); return 1;
        }
        output = yyjson_write(doc, (round & 1) ? YYJSON_WRITE_PRETTY : 0, &written);
        if (!output) { yyjson_doc_free(doc); free(input); return 1; }
        copy = yyjson_read(output, written, 0);
        if (!copy || !visit(&after, yyjson_doc_get_root(copy), 0) ||
            !same(before, after) || (round && !same(before, expected))) {
            fprintf(stderr, "roundtrip mismatch\n");
            yyjson_doc_free(copy); free(output); yyjson_doc_free(doc); free(input); return 1;
        }
        expected = before;
        yyjson_doc_free(copy); free(output); yyjson_doc_free(doc);
    }
    free(input);
    printf("{\"hash\":\"%016llx\",\"nodes\":%llu,\"string_bytes\":%llu,\"rounds\":%ld}\n",
           (unsigned long long)expected.hash, (unsigned long long)expected.nodes,
           (unsigned long long)expected.string_bytes, rounds);
    return 0;
}
