#include <limits.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/mman.h>
#include <unistd.h>
typedef unsigned long long U;
extern U positive(const int *, U), clamped(const int *, U), guarded(const int *, U);
extern U volatile_read(const volatile int *, U);
static U reference(const int *a, U n, unsigned kind) {
    U sum = 0;
    for (U i = 0; i < n; ++i) {
        int x = a[i];
        switch (kind) {
        case 0: sum += x > 0 ? (unsigned)x : 0; break;
        case 1: sum += x < -128 ? 0 : x > 127 ? 255 : (U)((int64_t)x + 128); break;
        case 2: sum += x == 0 ? 7 : (U)(128 / x); break;
        default: sum += x > 0 ? (unsigned)x : 0; break;
        }
    }
    return sum;
}
int main(void) {
    long page = sysconf(_SC_PAGESIZE);
    char *mem = mmap(0, (size_t)page * 3, PROT_NONE, MAP_PRIVATE | MAP_ANON, -1, 0);
    if (mem == MAP_FAILED || mprotect(mem + page, (size_t)page, PROT_READ | PROT_WRITE)) return 2;
    const int extrema[] = {INT_MIN, INT_MIN+1, -129, -128, -127, -1, 0, 1, 126, 127, 128, 129, INT_MAX-1, INT_MAX};
    U (*functions[])(const int *, U) = {positive, clamped, guarded};
    for (unsigned side = 0; side < 2; ++side) {
        for (U n = 0; n <= 257; ++n) {
            int *a = (int *)(mem + page + (side ? page - n * sizeof(int) : 0));
            for (U i = 0; i < n; ++i) a[i] = extrema[(i * 7 + n) % (sizeof(extrema)/sizeof(*extrema))];
            for (unsigned k = 0; k < 4; ++k) {
                U actual = k == 3 ? volatile_read(a,n) : functions[k](a,n);
                U expected = reference(a,n,k);
                if (actual != expected) {
                    fprintf(stderr,"side=%u n=%llu k=%u actual=%llu expected=%llu\n",side,n,k,actual,expected);
                    return 1;
                }
            }
        }
    }
    munmap(mem,(size_t)page*3);
    return 0;
}
