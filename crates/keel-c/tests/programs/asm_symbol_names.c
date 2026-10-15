/* C identifiers may also be Intel assembler keywords or register names. */
#include <stdio.h>

static long qword = 7;
static _Thread_local long byte = 11;

static long offset(long x) { return x + 1; }
static long rax(long x) { return x * 3; }
static long ptr(long x) { return x - 2; }

static long (*saved)(long) = offset;

int main(void) {
    long (*function)(long) = offset;
    printf("%ld %ld %ld\n", offset(rax(ptr(5))), function(13), saved(qword + byte));
    return 0;
}
