#include <assert.h>
#include <stdio.h>
typedef unsigned long long U;
U linkage_left(U), linkage_right(U), linkage_unused(U);
int main(void) {
    U inputs[] = {0, 1, 7, 255, 0x123456789abcdef0ULL, ~0ULL};
    for (unsigned i = 0; i < sizeof(inputs)/sizeof(*inputs); ++i) {
        U x = inputs[i];
        assert(linkage_left(x) == x * 32);
        assert(linkage_right(x) == (x ^ 16));
        assert(linkage_unused(x) == (x ^ 91));
        printf("%llu %llu\n", linkage_left(x), linkage_right(x));
    }
}
