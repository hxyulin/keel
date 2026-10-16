/* Separate TU, no LTO. Run against Keel explicit-pass and Clang objects.
 * No timing. Every nonempty array ends immediately before a protected page. */
#include <assert.h>
#include <limits.h>
#include <stdint.h>
#include <stdio.h>
#include <sys/mman.h>
#include <unistd.h>
extern unsigned long hot64(const unsigned long *, unsigned long, unsigned long, unsigned long);
extern unsigned hot32(const unsigned *, unsigned, unsigned, unsigned);
int main(void) {
    const unsigned counts[] = {0, 1, 2, 3, 4, 7, 16, 31, 64, 257};
    const unsigned long seeds[] = {0, 1, ULONG_MAX, ULONG_MAX - 7, 0x8123456789abcdefUL};
    long page = sysconf(_SC_PAGESIZE);
    assert(page > 0 && 257 * sizeof(unsigned long) <= (unsigned long)page);
    unsigned char *mem = mmap(0, (size_t)page * 2, PROT_READ | PROT_WRITE,
                              MAP_PRIVATE | MAP_ANON, -1, 0);
    assert(mem != MAP_FAILED);
    assert(mprotect(mem + page, (size_t)page, PROT_NONE) == 0);
    for (unsigned c = 0; c < sizeof(counts) / sizeof(counts[0]); ++c) {
        unsigned n = counts[c];
        unsigned long *p64 = (unsigned long *)(mem + page) - n;
        for (unsigned i = 0; i < n; ++i) p64[i] = ULONG_MAX - i * 0x102030405UL;
        for (unsigned s = 0; s < sizeof(seeds) / sizeof(seeds[0]); ++s) {
            unsigned long k = seeds[(s + 1) % 5], want = seeds[s];
            for (unsigned i = 0; i < n; ++i) want += (p64[i] ^ k) * (i + 1UL);
            assert(hot64(p64, n, k, seeds[s]) == want);
            if (!n) assert(hot64(0, 0, k, seeds[s]) == want);
        }
        unsigned *p32 = (unsigned *)(mem + page) - n;
        for (unsigned i = 0; i < n; ++i) p32[i] = UINT_MAX - i * 0x1020304U;
        for (unsigned s = 0; s < sizeof(seeds) / sizeof(seeds[0]); ++s) {
            unsigned k = (unsigned)seeds[(s + 1) % 5], want = (unsigned)seeds[s];
            for (unsigned i = 0; i < n; ++i) want += (p32[i] ^ k) * (i + 1U);
            assert(hot32(p32, n, k, (unsigned)seeds[s]) == want);
            if (!n) assert(hot32(0, 0, k, (unsigned)seeds[s]) == want);
        }
    }
    assert(munmap(mem, (size_t)page * 2) == 0);
    puts("loop-unroll: all bounded wrapping/guard-page cases passed");
}
