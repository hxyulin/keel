#include <stddef.h>
#include <stdint.h>
#include <limits.h>
#include <stdbool.h>
#include <float.h>
#include <stdalign.h>
#include <stdarg.h>
_Static_assert(__STDC_HOSTED__ == 0, "freestanding");
_Static_assert(sizeof(long) == 4 && sizeof(void *) == 4, "ILP32");
_Static_assert(sizeof(size_t) == 4 && sizeof(ptrdiff_t) == 4, "pointer integers");
_Static_assert(sizeof(int64_t) == 8 && sizeof(intmax_t) == 8, "wide integers");
_Static_assert(_Generic((size_t)0, unsigned long: 1, default: 0), "size identity");
_Static_assert(_Generic((ptrdiff_t)0, long: 1, default: 0), "difference identity");
_Static_assert(_Generic(INT64_C(1), long long: 1, default: 0), "LL suffix");
_Static_assert(_Generic(UINTMAX_C(1), unsigned long long: 1, default: 0), "ULL suffix");
_Static_assert(sizeof(va_list) == 4 && alignof(va_list) == 4, "va declaration");
_Static_assert(sizeof(long double) == 16 && alignof(long double) == 16, "binary128 declaration");
_Static_assert(alignof(max_align_t) == 16, "max alignment");
_Static_assert(CHAR_MIN == -128 && CHAR_MAX == 127, "signed char");
_Static_assert(LONG_MAX == 2147483647L && ULONG_MAX == 4294967295UL, "long limits");
_Static_assert(SIZE_MAX == 4294967295UL && PTRDIFF_MAX == 2147483647L, "pointer limits");
_Static_assert(LDBL_MANT_DIG == 113 && LDBL_MAX_EXP == 16384, "binary128 limits");
_Static_assert(FLT_MANT_DIG == 24 && DBL_MANT_DIG == 53, "FP declarations");
typedef int Word __attribute__((mode(word)));
typedef unsigned int Pointer __attribute__((mode(pointer)));
typedef unsigned int DoubleWord __attribute__((mode(DI)));
_Static_assert(sizeof(Word) == 4 && sizeof(Pointer) == 4 && sizeof(DoubleWord) == 8, "modes");
struct Record { char a; void *b; long c; };
_Static_assert(sizeof(struct Record) == 12 && offsetof(struct Record, c) == 8, "record");
static int numbers[3] = {11, 22, 33};
static int *link = numbers + 1;
static unsigned long helper(unsigned long x) { return x * 3 + 1; }
unsigned long checksum(unsigned int address, int length) {
    unsigned char *input = (unsigned char *)(uintptr_t)address;
    unsigned char local[16];
    unsigned long sum = 7;
    for (int i = 0; i < 16; i++) local[i] = (unsigned char)(i ^ 0x5a);
    for (int i = 0; i < length; i++) sum = (sum * 33) ^ input[i];
    for (int i = 0; i < 16; i++) sum ^= (unsigned long)local[i] << (i & 7);
    return sum + helper(length) + *link;
}
int local_array(void) { int a[4] = {7, 0, 3, 4}; return a[0]+a[1]+a[2]+a[3]; }
