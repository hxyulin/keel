/* Fixed-type overflow families: boundary values, stores and evaluation. */
#include <limits.h>
#include <stdio.h>

#define DEFINE_CASES(T, P, S, MINIMUM, MAXIMUM) \
static void check_##P##S(void) { \
    T pairs[][2] = { \
        { MINIMUM, -1 }, { MINIMUM, 1 }, { MAXIMUM, 1 }, \
        { MAXIMUM, 2 }, { MAXIMUM, -1 }, { MINIMUM, MAXIMUM }, \
        { 0, 0 }, { 0, MAXIMUM }, { 7, 3 }, { -7, -3 } \
    }; \
    for (unsigned i = 0; i < sizeof pairs / sizeof pairs[0]; i++) { \
        T a = pairs[i][0], b = pairs[i][1], result = 123; \
        int flag = __builtin_##P##add##S##_overflow(a, b, &result); \
        printf(#P #S " add %u %d %llx\n", i, flag, (unsigned long long)result); \
        flag = __builtin_##P##sub##S##_overflow(a, b, &result); \
        printf(#P #S " sub %u %d %llx\n", i, flag, (unsigned long long)result); \
        flag = __builtin_##P##mul##S##_overflow(a, b, &result); \
        printf(#P #S " mul %u %d %llx\n", i, flag, (unsigned long long)result); \
    } \
}

DEFINE_CASES(int, s, , INT_MIN, INT_MAX)
DEFINE_CASES(unsigned int, u, , 0, UINT_MAX)
DEFINE_CASES(long, s, l, LONG_MIN, LONG_MAX)
DEFINE_CASES(unsigned long, u, l, 0, ULONG_MAX)
DEFINE_CASES(long long, s, ll, LLONG_MIN, LLONG_MAX)
DEFINE_CASES(unsigned long long, u, ll, 0, ULLONG_MAX)

static int left_calls, right_calls, pointer_calls, stored;
static int left(void) { left_calls++; return INT_MAX; }
static int right(void) { right_calls++; return 1; }
static int *destination(void) { pointer_calls++; return &stored; }

int main(void) {
    check_s(); check_u(); check_sl(); check_ul(); check_sll(); check_ull();

    int flag = __builtin_sadd_overflow(left(), right(), destination());
    printf("once %d %d %d %d %d\n", flag, stored, left_calls, right_calls, pointer_calls);
    /* The store is observable even when the overflow flag is unused. */
    __builtin_ssub_overflow(8, 3, &stored);
    printf("unused flag %d\n", stored);
    unsigned converted;
    flag = __builtin_uadd_overflow(-1, 1, &converted);
    printf("convert %d %u\n", flag, converted);
    flag = __builtin_sadd_overflow(1.5, 2.5, (void *)&stored);
    printf("prototype %d %d\n", flag, stored);
    printf("boolean %d\n", _Generic(__builtin_sadd_overflow(1, 2, &stored),
                                      _Bool: 1, default: 0));
    return 0;
}
