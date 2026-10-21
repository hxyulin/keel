#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
extern uint64_t cpp_case(uint64_t, uint64_t, volatile uint64_t *);
int main(void) {
    const uint64_t values[] = {0, 1, 2, 65534, 65535, 65536, UINT64_MAX, UINT64_C(0x8000000000000000)};
    for (unsigned i = 0; i < sizeof(values)/sizeof(values[0]); ++i) {
        uint64_t v = values[i] & 65535;
        uint64_t got = cpp_case(values[i], 0, 0);
        if (got != (v + 1) * (v + 3)) abort();
        printf("%llu\n", (unsigned long long)got);
    }
}
