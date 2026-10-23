/* Common Clang driver, linked without LTO. Inputs/oracle supplied by Python. */
#define _POSIX_C_SOURCE 200809L
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
extern void kernel(void *, const void *, const void *);
typedef struct { _Alignas(16) unsigned char a[16], b[16], expected[16]; } Row;
static Row rows[128];
static volatile uint64_t sink;
static double now(void) {
    struct timespec t;
    if (clock_gettime(CLOCK_MONOTONIC, &t)) exit(5);
    return t.tv_sec + t.tv_nsec * 1e-9;
}
int main(int argc, char **argv) {
    if (argc != 2 && argc != 3) return 1;
    unsigned long reps = strtoul(argv[1], 0, 10);
    size_t n = fread(rows, sizeof(Row), 128, stdin);
    if (!n || ferror(stdin) || !reps) return 2;
    _Alignas(16) unsigned char output[48], a[16], b[16];
    for (size_t i = 0; i < n; ++i) {
        memset(output, 0xa5, sizeof(output));
        kernel(output+16, rows[i].a, rows[i].b);
        if (memcmp(output+16, rows[i].expected, 16)) return 3;
        for (int j = 0; j < 16; ++j)
            if (output[j] != 0xa5 || output[j+32] != 0xa5) return 4;
        /* Exact aliases must load operands before publishing the output. */
        memcpy(a, rows[i].a, 16); memcpy(b, rows[i].b, 16);
        kernel(a, a, b);
        if (memcmp(a, rows[i].expected, 16)) return 3;
        memcpy(a, rows[i].a, 16); memcpy(b, rows[i].b, 16);
        kernel(b, a, b);
        if (memcmp(b, rows[i].expected, 16)) return 3;
    }
    if (argc == 3) {
        /* Explicit IR align=1 only; C vector dereferences require alignment. */
        unsigned char ua[32], ub[32], ud[48];
        for (size_t i = 0; i < n; ++i) {
            memcpy(ua+1, rows[i].a, 16); memcpy(ub+3, rows[i].b, 16);
            memset(ud, 0xa5, sizeof(ud));
            kernel(ud+5, ua+1, ub+3);
            if (memcmp(ud+5, rows[i].expected, 16)) return 3;
            for (int j = 0; j < 48; ++j)
                if ((j < 5 || j >= 21) && ud[j] != 0xa5) return 4;
        }
    }
    uint64_t checksum = 0;
    double start = now();
    for (unsigned long r = 0; r < reps; ++r)
        for (size_t i = 0; i < n; ++i) {
            uint64_t word;
            kernel(output+16, rows[i].a, rows[i].b);
            memcpy(&word, output+16, 8);
            checksum += word;
            sink = checksum;
        }
    double seconds = now()-start;
    uint64_t expected = 0;
    for (size_t i = 0; i < n; ++i) {
        uint64_t word; memcpy(&word, rows[i].expected, 8); expected += word;
    }
    if (checksum != expected * reps) return 6;
    printf("{\"seconds\":%.9f,\"checksum\":\"%016llx\",\"calls\":%llu}\n",
           seconds, (unsigned long long)checksum, (unsigned long long)(n*reps));
    return 0;
}
