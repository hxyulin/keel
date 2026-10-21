/* Correctness only. No timing; portable macOS ARM / Linux x86 guard pages. */
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>
#ifndef MAP_ANON
#define MAP_ANON MAP_ANONYMOUS
#endif
extern uint64_t rk_clamped(const void *, const void *, const void *, uint64_t);

int main(void) {
    size_t page = (size_t)sysconf(_SC_PAGESIZE);
    unsigned char *p = mmap(0, 3 * page, PROT_NONE, MAP_PRIVATE | MAP_ANON, -1, 0);
    if (p == MAP_FAILED || mprotect(p + page, page, PROT_READ | PROT_WRITE)) return 2;
    unsigned char *end = p + 2 * page;
    uint32_t edges[] = {0, 1, UINT32_MAX, INT32_MAX, UINT32_C(0x80000000),
        UINT32_C(0x80000001), (uint32_t)-129, (uint32_t)-128, (uint32_t)-127,
        126, 127, 128, UINT32_C(0xffffff00), UINT32_C(0xffffff01)};
    unsigned count = sizeof(edges) / sizeof(edges[0]);
    unsigned checks = 0;
    for (unsigned n = 0; n <= 129; ++n)
    for (unsigned offset = 0; offset < 4; ++offset)
    for (unsigned pattern = 0; pattern < count + 2; ++pattern) {
        unsigned char *a = end - 4 * n - offset;
        uint64_t expected = 0;
        for (unsigned i = 0; i < n; ++i) {
            uint32_t bits = pattern < count ? edges[pattern] : pattern == count
                ? edges[i % count] : i * UINT32_C(0x9e3779b9) + 17;
            memcpy(a + 4 * i, &bits, 4);
            int32_t x; memcpy(&x, &bits, 4);
            expected += x < -128 ? 0 : x > 127 ? 255 : (uint64_t)(x + 128);
        }
        uint64_t actual = rk_clamped(a, p, end, n);
        ++checks;
        if (actual != expected) {
            fprintf(stderr, "n=%u offset=%u pattern=%u got=%llu expected=%llu\n",
                n, offset, pattern, (unsigned long long)actual, (unsigned long long)expected);
            return 1;
        }
    }
    /* Neither input nor unused ABI streams may be touched for zero trips. */
    if (rk_clamped(p, p, p, 0) || rk_clamped(end, end, end, 0)) return 3;
    if (munmap(p, 3 * page)) return 4;
    printf("{\"status\":\"passed\",\"oracle_calls\":%u,\"lengths\":[0,129],"
        "\"unaligned_offsets\":[0,3],\"guard_pages\":true,\"timing\":false}\n", checks + 2);
    return 0;
}
