#include <assert.h>
#include <stdio.h>
unsigned long long available_entry(unsigned long long);
int main(void) {
    for (unsigned long long x = 0; x < 100; ++x) {
        assert(available_entry(x) == (x ^ 23));
        printf("%llu\n", available_entry(x));
    }
}
