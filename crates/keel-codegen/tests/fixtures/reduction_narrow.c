#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>

extern int64_t k_narrow(const int16_t *, uint64_t);

static int64_t expected(const int16_t *p, uint64_t n) {
    int64_t sum = 0;
    for (uint64_t i = 0; i < n; ++i) {
        int32_t v = p[i];
        int32_t clamped = v < -128 ? -128 : v > 127 ? 127 : v;
        int32_t high_byte = v >= 0 ? v / 256 : -((255 - v) / 256);
        uint16_t mixed = (uint16_t)((uint8_t)v * 3 + high_byte * 5 + ((uint16_t)v >> 2));
        int32_t product = (uint32_t)(v * 3) & 65535u;
        if (product >= 32768) product -= 65536;
        int32_t shifted = v >= 0 ? v / 8 : -((7 - v) / 8);
        sum += clamped * 7 + mixed - product / 7 + (uint8_t)shifted;
    }
    return sum;
}

int main(void) {
    const size_t page = (size_t)sysconf(_SC_PAGESIZE);
    const size_t bytes = 65536 * sizeof(int16_t);
    if (!page || bytes % page) return 2;
    uint8_t *region = mmap(NULL, bytes + page, PROT_READ | PROT_WRITE,
                          MAP_PRIVATE | MAP_ANON, -1, 0);
    if (region == MAP_FAILED || mprotect(region + bytes, page, PROT_NONE)) return 3;
    const uint64_t lengths[] = {0, 1, 2, 3, 7, 8, 9, 15, 16, 17, 31, 32, 33, 65, 65536};
    for (unsigned mode = 0; mode < 3; ++mode) {
        for (unsigned j = 0; j < sizeof(lengths)/sizeof(lengths[0]); ++j) {
            const uint64_t n = lengths[j];
            int16_t *p = (int16_t *)(region + bytes) - n;
            for (uint64_t i = 0; i < n; ++i) {
                uint32_t index = mode == 0 ? (uint32_t)i : mode == 1 ? 65535u - (uint32_t)i : (uint32_t)(i * 40503u + 32767u) & 65535u;
                p[i] = (int16_t)((int32_t)index - 32768);
            }
            const int64_t want = expected(p, n);
            const int64_t got = k_narrow(p, n);
            if (got != want) {
                fprintf(stderr, "narrow mismatch at length %llu, mode %u: %lld != %lld\n",
                        (unsigned long long)n, mode, (long long)got, (long long)want);
                return 1;
            }
        }
    }
    // A constant vector tests each i16 contribution independently. A single
    // exhaustive sum could hide equal and opposite arithmetic errors.
    int16_t *lanes = (int16_t *)(region + bytes) - 8;
    for (int32_t v = -32768; v <= 32767; ++v) {
        for (unsigned i = 0; i < 8; ++i) lanes[i] = (int16_t)v;
        if (k_narrow(lanes, 8) != expected(lanes, 8)) {
            fprintf(stderr, "narrow mismatch for repeated value %d\n", v);
            return 1;
        }
    }
    return munmap(region, bytes + page) != 0;
}
