#include <stdint.h>
#include <stddef.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>

extern uint32_t read_pair32(const unsigned char *);
extern uint64_t read_pair64(const unsigned char *);
extern void copy_captures(uintptr_t *, const uintptr_t *, size_t,
                          const uintptr_t *, size_t);

int main(void) {
    long page = sysconf(_SC_PAGESIZE);
    unsigned char *mem = mmap(0, 2 * page, PROT_READ | PROT_WRITE,
                              MAP_PRIVATE | MAP_ANON, -1, 0);
    if (mem == MAP_FAILED || mprotect(mem + page, page, PROT_NONE)) return 1;
    for (unsigned v = 0; v < 65536; v++) {
        mem[1] = (unsigned char)v;
        mem[2] = (unsigned char)(v >> 8);
        if (read_pair32(mem + 1) != v || read_pair64(mem + 1) != v) return 2;
    }
    mem[page - 2] = 0x34;
    mem[page - 1] = 0x92;
    if (read_pair32(mem + page - 2) != 0x9234 ||
        read_pair64(mem + page - 2) != 0x9234) return 3;
    munmap(mem, 2 * page);

    const size_t sizes[] = {0, 1, 7, 8, 9, 16, 17, 32};
    uintptr_t got[192], want[192], stack[32];
    for (size_t i = 0; i < 32; i++) stack[i] = i * 257 + 19;
    for (size_t k = 0; k < sizeof(sizes) / sizeof(sizes[0]); k++) {
        size_t n = sizes[k];
        for (size_t m = 0; m <= 4; m += 4) {
            for (int delta = -3; delta <= 3; delta += 3) {
                for (size_t i = 0; i < 192; i++) got[i] = want[i] = i * 137 + 11;
                volatile uintptr_t *d = want + 64 + delta;
                const volatile uintptr_t *s = want + 64;
                for (size_t i = 0; i < n; i++) d[i] = s[i];
                for (size_t i = 0; i < m; i++) d[n + i] = stack[i];
                copy_captures(got + 64 + delta, got + 64, n, stack, m);
                if (memcmp(got, want, sizeof(got))) return 4;
            }
        }
    }
    /* Empty copies must not read either source. */
    copy_captures(got, 0, 0, 0, 0);
    return 0;
}
