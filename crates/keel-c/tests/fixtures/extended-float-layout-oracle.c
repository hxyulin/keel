/* Native x86-64 Linux oracle, not a claim of Keel value/ABI support.
 * Run with genuine GCC and Clang; see docs/c-system-header-compat.md.
 * No system or application headers are rewritten by this reduction. */
#include <stddef.h>
#include <string.h>

typedef _Complex float TC __attribute__((__mode__(__TC__)));
typedef _Complex long double TC_from_ld __attribute__((mode(TC)));
typedef _Complex __attribute__((mode(TC))) long double TC_among;
_Static_assert(__builtin_types_compatible_p(TC, TC_from_ld), "long double mode subject");
_Static_assert(__builtin_types_compatible_p(TC, TC_among), "specifier mode subject");
struct wrapped_tc { char before; TC value; char after; };
_Static_assert(sizeof(TC) == 32, "TC size");
_Static_assert(_Alignof(TC) == 16, "TC alignment is not its size");
_Static_assert(offsetof(struct wrapped_tc, value) == 16, "member alignment");
_Static_assert(offsetof(struct wrapped_tc, after) == 48, "exact member footprint");
_Static_assert(sizeof(struct wrapped_tc) == 64, "tail padding");
_Static_assert(sizeof(TC[2]) == 64, "array stride");
_Static_assert(!__builtin_types_compatible_p(TC, _Complex float), "float identity");
_Static_assert(!__builtin_types_compatible_p(TC, _Complex double), "double identity");
_Static_assert(!__builtin_types_compatible_p(TC, _Complex long double), "x87 identity");

#ifndef __clang__
#define CHECK_TYPE(T, S, A) \
    _Static_assert(sizeof(T) == S && _Alignof(T) == A, #T " layout"); \
    _Static_assert(!__builtin_types_compatible_p(T, float), #T " vs float"); \
    _Static_assert(!__builtin_types_compatible_p(T, double), #T " vs double"); \
    _Static_assert(!__builtin_types_compatible_p(T, long double), #T " vs long double")
CHECK_TYPE(_Float32, 4, 4);
CHECK_TYPE(_Float64, 8, 8);
CHECK_TYPE(_Float32x, 8, 8);
CHECK_TYPE(_Float64x, 16, 16);
CHECK_TYPE(_Float128, 16, 16);
_Static_assert(!__builtin_types_compatible_p(_Float64, _Float32x), "equal encoding is not identity");
_Static_assert(__builtin_types_compatible_p(_Float128, __float128), "GNU binary128 alias");
_Static_assert(__builtin_types_compatible_p(TC, _Complex _Float128), "TC canonical identity");
_Static_assert(_Generic((_Float32)0, _Float32: 1, float: 0, default: 0), "generic identity");
#endif

int main(void) {
    /* Check the specified two-component memory representation, including all
     * bytes and surrounding sentinels; this does not test arithmetic or calls. */
    struct wrapped_tc object;
    memset(&object, 0xa5, sizeof(object));
    TC value = 1.0 + 2.0i;
    __float128 parts[2] = {1.0, 2.0};
    memcpy(&object.value, &value, sizeof(value));
    return memcmp(&object.value, parts, sizeof(value)) != 0 ||
        (unsigned char)object.before != 0xa5 ||
        (unsigned char)object.after != 0xa5;
}
