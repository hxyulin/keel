#include <stdint.h>
#include <stdio.h>

extern int64_t add_masked(int64_t, int64_t);
extern int64_t phi_masked(int64_t, int64_t);
extern int64_t nonnull_masked(const int64_t *, int64_t);
extern int64_t nonnull_null_masked(void);
extern uint64_t shift_masked(uint64_t, uint64_t, uint64_t);

static int check(uint64_t actual, uint64_t expected, unsigned index) {
    if (actual == expected) return 0;
    fprintf(stderr, "definedness case %u: got %llu, expected %llu\n", index,
            (unsigned long long)actual, (unsigned long long)expected);
    return 1;
}

int main(void) {
    unsigned index = 0;
    const int64_t inputs[] = {INT64_MIN, -1, 0, 1, INT64_MAX - 1, INT64_MAX};
    for (unsigned i = 0; i < sizeof(inputs) / sizeof(inputs[0]); ++i) {
        int64_t x = inputs[i];
        if (check((uint64_t)add_masked(x, 0), 7, index++)) return 1;
        if (check((uint64_t)phi_masked(x, 0), 7, index++)) return 1;
        if (x != INT64_MAX) {
            if (check((uint64_t)add_masked(x, 1), (uint64_t)(x + 1), index++)) return 1;
            if (check((uint64_t)phi_masked(x, 1), (uint64_t)(x + 1), index++)) return 1;
        }
    }
    int64_t object = 37;
    if (check((uint64_t)nonnull_null_masked(), 91, index++)) return 1;
    if (check((uint64_t)nonnull_masked(&object, 0), 91, index++)) return 1;
    if (check((uint64_t)nonnull_masked(&object, 1), 37, index++)) return 1;
    const uint64_t amounts[] = {0, 1, 63, 64, 65, UINT64_MAX};
    for (unsigned i = 0; i < sizeof(amounts) / sizeof(amounts[0]); ++i) {
        uint64_t amount = amounts[i];
        if (check(shift_masked(13, amount, 0), 7, index++)) return 1;
        if (amount < 64 && check(shift_masked(13, amount, 1), UINT64_C(13) << amount, index++)) return 1;
    }
    puts("definedness-ok");
    return 0;
}
