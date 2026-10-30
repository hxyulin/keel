#include <stdint.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>
#define DECL(name) extern void name(const void *, void *, void *);
DECL(zmm_add) DECL(zmm_fixed) DECL(zmm_early)
DECL(high_xmm) DECL(high_ymm) DECL(zmm_xmm_clobber)
DECL(zmm_ymm_clobber) DECL(zmm_zmm_clobber) DECL(zmm_mixed)
int main(void) {
    long page = sysconf(_SC_PAGESIZE);
    unsigned char *a = mmap(0, page * 2, PROT_NONE, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    unsigned char *b = mmap(0, page * 2, PROT_NONE, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    unsigned char *c = mmap(0, page * 2, PROT_NONE, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (a == MAP_FAILED || b == MAP_FAILED || c == MAP_FAILED ||
        mprotect(a, page, PROT_READ | PROT_WRITE) ||
        mprotect(b, page, PROT_READ | PROT_WRITE) ||
        mprotect(c, page, PROT_READ | PROT_WRITE)) return 1;
    void (*ops[])(const void *, void *, void *) = {
        zmm_add,zmm_fixed,zmm_early,high_xmm,high_ymm,
        zmm_xmm_clobber,zmm_ymm_clobber,zmm_zmm_clobber
    };
    for (unsigned f = 0; f < 8; f++) {
        unsigned bytes = f == 3 ? 16 : f == 4 ? 32 : 64;
        for (unsigned i = 0; i < bytes / 4; i++) {
            uint32_t x = UINT32_C(0x8123feda) + i * UINT32_C(0x7123415);
            memcpy(a + page - bytes + i * 4, &x, 4);
        }
        memset(b, 0xa5, page); memset(c, 0xa5, page);
        ops[f](a + page - bytes, b + page - bytes, c + page - bytes);
        for (unsigned i = 0; i < bytes / 4; i++) {
            uint32_t input, actual;
            memcpy(&input, a + page - bytes + i * 4, 4);
            memcpy(&actual, b + page - bytes + i * 4, 4);
            if (actual != (f < 3 ? input + input : input)) return 10 + f;
        }
        if (memcmp(a + page - bytes, c + page - bytes, bytes) ||
            b[page - bytes - 1] != 0xa5 || c[page - bytes - 1] != 0xa5) return 20 + f;
    }
    memset(b, 0xa5, page); memset(c, 0xa5, page);
    zmm_mixed(a + page - 64, b + page - 112, c + page - 112);
    unsigned offsets[] = {0,16,48}, widths[] = {16,32,64};
    for (unsigned i = 0; i < 3; i++)
        if (memcmp(a + page - 64, b + page - 112 + offsets[i], widths[i]) ||
            memcmp(a + page - 64, c + page - 112 + offsets[i], widths[i])) return 30;
    if (b[page - 113] != 0xa5 || c[page - 113] != 0xa5) return 31;
    return 0;
}
