#include <stddef.h>
#include <stdint.h>

/* QuickJS's little-endian byte assembly, without alignment assumptions. */
uint32_t read_pair32(const unsigned char *p) {
    return p[0] | (uint32_t)p[1] << 8;
}

uint64_t read_pair64(const unsigned char *p) {
    return p[0] | (uint64_t)p[1] << 8;
}

/* QuickJS push_state's two ordered loops after capacity and header setup. */
void copy_captures(uintptr_t *dst, const uintptr_t *captures, size_t n,
                   const uintptr_t *stack, size_t m) {
    for (size_t i = 0; i < n; i++) dst[i] = captures[i];
    uintptr_t *tail = dst + n;
    for (size_t i = 0; i < m; i++) tail[i] = stack[i];
}
