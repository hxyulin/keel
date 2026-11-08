#include <stdint.h>
#include <stdio.h>

int64_t external_value = 11;
extern int64_t seed, counter;
extern int64_t system_run(int64_t *, int64_t (*)(int64_t), int64_t);
extern int64_t use_pointer_global(int64_t), use_function_address(int64_t);
extern void *roundtrip(void *);
extern int32_t is_null(void *);
extern int8_t echo_signed(int8_t);
extern uint16_t echo_unsigned(uint16_t);
extern int64_t call_host_byte(void), many_arguments(void);
extern int64_t sum_loop(const int64_t *, int64_t);
extern int64_t (* const helper_pointer)(int64_t);

static int touches;
void host_touch(int64_t *p) { *p += 3; ++touches; }
int64_t host_adjust(int8_t a, uint16_t extra) { return (int64_t)a + extra; }
int8_t host_byte(void) { return -101; }
int64_t host_many(int64_t a, int64_t b, int64_t c, int64_t d, int64_t e,
                  int64_t f, int64_t g, int64_t h, int64_t i, int64_t j) {
    return a + 2*b + 3*c + 4*d + 5*e + 6*f + 7*g + 8*h + 9*i + 10*j;
}
static int64_t callback(int64_t x) { return x + 100; }

int main(void) {
    int64_t array[] = { 130, 4 };
    if (seed != 7 || counter != 0) return 1;
    if (system_run(array, callback, 10) != 65691 || array[1] != 134) return 2;
    if (system_run(array, callback, -2) != 65798 || array[1] != 264) return 3;
    if (touches != 2 || counter != 2 || seed != 7) return 4;
    if (use_pointer_global(9) != 18 || use_function_address(-9) != -18 || helper_pointer(7) != 14) return 5;
    if (roundtrip(array) != array || roundtrip(NULL) != NULL) return 6;
    if (is_null(array) != 0 || is_null(NULL) != 1) return 7;
    if (echo_signed(-128) != -128 || echo_signed(127) != 127) return 8;
    if (echo_unsigned(65530) != 65530 || echo_unsigned(0) != 0) return 9;
    if (call_host_byte() != -101 || many_arguments() != 385) return 10;
    if (sum_loop(array, 2) != 394 || sum_loop(NULL, 0) != 0 || sum_loop(NULL, -1) != 0) return 11;
    puts("systems API passed");
    return 0;
}
