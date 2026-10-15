/* Hints preserve values, result types and all runtime argument effects. */
#include <stdio.h>

static long buffer[8];
static int value_calls, expected_calls, pointer_calls, offset_calls;
static long value(void) { value_calls++; return -7; }
static long expected(void) { expected_calls++; return 1; }
static void *pointer(void) { pointer_calls++; return buffer; }
static unsigned long offset(void) { offset_calls++; return 3; }

static long constant = __builtin_expect(12, 1);
static void *constant_pointer = __builtin_assume_aligned(buffer, 8);

int main(void) {
    long result = __builtin_expect(value(), expected());
    printf("expect %ld %d %d\n", result, value_calls, expected_calls);
    result = __builtin_expect_with_probability(value(), expected(), 0.75);
    printf("probability %ld %d %d\n", result, value_calls, expected_calls);
    result = __builtin_expect_with_probability(11U, 1, 0.0);
    printf("zero %ld\n", result);
    result = __builtin_expect_with_probability(-2, 0, 1);
    printf("one %ld\n", result);
    result = __builtin_expect_with_probability(8, 1, (0.25 + 0.25));
    printf("expression %ld\n", result);
    result = __builtin_expect_with_probability(4, 1, 1 ? 0.5 : value());
    printf("unevaluated %ld %d\n", result, value_calls);
    result = __builtin_expect_with_probability(5, 1, (1, 0.5));
    printf("comma %ld\n", result);

    __auto_type aligned = __builtin_assume_aligned(pointer(), sizeof(long));
    printf("alignment %d %d %d\n", aligned == buffer, pointer_calls,
           _Generic(aligned, void *: 1, default: 0));
    void *shifted = (char *)pointer() + 3;
    void *same = __builtin_assume_aligned(shifted, 8, offset());
    printf("offset %d %d %d\n", same == shifted, pointer_calls, offset_calls);
    same = __builtin_assume_aligned(shifted, 8, offset());
    printf("offset twice %d %d\n", same == shifted, offset_calls);
    same = __builtin_assume_aligned((void *)0, 1);
    printf("null %d\n", same == 0);
    printf("constants %ld %d\n", constant, constant_pointer == buffer);
    printf("long type %d\n", _Generic(__builtin_expect_with_probability(1, 1, 0.5),
                                       long: 1, default: 0));
    return 0;
}
