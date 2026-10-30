#include <stdint.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>

extern void header_map(uint32_t *, const uint32_t *, int64_t);

static void oracle(uint32_t *dst, const uint32_t *src, int64_t start) {
    for (int64_t i = start; i < 17; ++i)
        dst[i] = (dst[i] + src[i]) ^ 7;
}

int main(void) {
    const int offsets[] = {16, 15, 17, 48};
    const int starts[] = {-1, 0, 3, 17, 18};
    for (unsigned alias = 0; alias < sizeof(offsets) / sizeof(offsets[0]); ++alias) {
        for (unsigned start = 0; start < sizeof(starts) / sizeof(starts[0]); ++start) {
            uint32_t actual[80], expected[80];
            for (unsigned i = 0; i < 80; ++i)
                actual[i] = UINT32_C(0x9e3779b9) * i + UINT32_C(0xfedcba98);
            memcpy(expected, actual, sizeof(actual));
            oracle(expected + 16, expected + offsets[alias], starts[start]);
            header_map(actual + 16, actual + offsets[alias], starts[start]);
            if (memcmp(actual, expected, sizeof(actual))) return 1;
        }
    }
    /* A literal bound must not let a vector read consume the scalar tail. */
    long page = sysconf(_SC_PAGESIZE);
    if (page < 4096) return 2;
    unsigned char *region = mmap(0, 2 * (size_t)page, PROT_READ | PROT_WRITE,
                                 MAP_PRIVATE | MAP_ANON, -1, 0);
    if (region == MAP_FAILED || mprotect(region + page, (size_t)page, PROT_NONE)) return 3;
    uint32_t *src = (uint32_t *)(region + page) - 17;
    uint32_t actual[17], expected[17];
    for (unsigned i = 0; i < 17; ++i) src[i] = actual[i] = expected[i] = UINT32_MAX - i;
    oracle(expected, src, 0);
    header_map(actual, src, 0);
    if (memcmp(actual, expected, sizeof(actual))) return 4;
    header_map(0, 0, 17);
    header_map(0, 0, 18);
    munmap(region, 2 * (size_t)page);
    return 0;
}
