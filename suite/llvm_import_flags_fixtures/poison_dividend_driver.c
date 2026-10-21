#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
extern uint64_t masked_add_div(uint64_t, uint64_t), unused_add_div(uint64_t);
extern uint64_t masked_add_rem(uint64_t, uint64_t), masked_exact_div(uint64_t, uint64_t);
extern uint64_t zero_divisor(uint64_t, uint64_t);
static void check(uint64_t got, uint64_t expected) {
    if (got != expected) abort();
    printf("%llu\n", (unsigned long long)got);
}
int main(int argc, char **argv) {
    if (argc > 1) { volatile uint64_t z = 0; return (int)zero_divisor(7, z); }
    check(masked_add_div(INT64_MAX, 0), 73);
    check(unused_add_div(INT64_MAX), 73);
    check(masked_add_rem(INT64_MAX, 0), 73);
    check(masked_exact_div(3, 0), 73);
    check(masked_add_div(0, 1), UINT64_MAX);
    check(masked_add_div(INT64_MIN, 1), INT64_MAX);
    check(masked_add_rem(0, 1), 0);
    check(masked_exact_div(0, 1), 0);
}
