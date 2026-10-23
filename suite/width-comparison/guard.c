#define _GNU_SOURCE
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>
extern unsigned long long rk_widen32(const int *, const long long *, const unsigned long long *, unsigned long long);
extern unsigned long long seeded(const int *, long long, long long, unsigned long long);
int main(void) {
    long page = sysconf(_SC_PAGESIZE);
    unsigned char *m = mmap(0, page * 3, PROT_NONE, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (m == MAP_FAILED || mprotect(m + page, page, PROT_READ | PROT_WRITE)) return 2;
    const int32_t edges[] = {INT32_MIN, INT32_MAX, -1, 0, 1, -1234567, 987654321};
    const uint64_t seeds[] = {0, UINT64_MAX, UINT64_C(0xfedcba9876543210)};
    unsigned cases = 0;
    for (unsigned n = 0; n <= 81; ++n)
        for (unsigned start = 0; start <= 5; ++start)
            for (unsigned edge = 0; edge < 18; ++edge) {
                unsigned char *p = edge == 0 ? m + page : edge == 1 ? m + 2 * page - (n + start) * 4 : m + page + edge - 2;
                uint64_t total = 0, slice = 0;
                for (unsigned i = 0; i < n + start; ++i) {
                    int32_t x = edges[i % 7];
                    memcpy(p + i * 4, &x, 4);
                    total += (int64_t)x;
                    if (i >= start) slice += (int64_t)x;
                }
                if (rk_widen32((const int *)p, 0, 0, n + start) != total) return 3;
                for (unsigned s = 0; s < 3; ++s) {
                    if (seeded((const int *)p, start, n + start, seeds[s]) != slice + seeds[s]) {
                        fprintf(stderr, "seeded n=%u start=%u edge=%u\n", n, start, edge); return 4;
                    }
                    ++cases;
                }
            }
    if (seeded((const int *)m, 17, 3, 123) != 123 || seeded((const int *)m, 0, -1, 456) != 456) return 5;
    int32_t x = -31;
    memcpy(m + page, &x, 4);
    if (seeded((const int *)(m + page + 4), -1, 0, 100) != 69) return 6;
    printf("{\"guard_cases\":%u}\n", cases);
    return munmap(m, page * 3) != 0;
}
