#include <stdint.h>
#include <stdio.h>

/* Same names as the keel module's local symbols. */
int64_t counter = 1;
_Thread_local int64_t tls_count = 2;
int64_t helper(int64_t x) { return 2 * x; }

int64_t k_linkage(int64_t x);

int main(void) {
    int failures = 0;
    int64_t first = k_linkage(1), second = k_linkage(1);
    if (first != 1001 + 100 + 7 || second != 1001 + 101 + 8) {
        printf("k_linkage gave %lld, %lld\n", (long long)first, (long long)second);
        failures++;
    }
    if (helper(3) != 6 || counter != 1 || tls_count != 2) {
        printf("the driver's own symbols changed\n");
        failures++;
    }
    return failures;
}
