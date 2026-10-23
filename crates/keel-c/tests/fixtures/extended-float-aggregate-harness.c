/* Link with extended-float-aggregate-abi.c compiled by each compiler.
 * Native GCC/Clang controls return zero. The Keel 8bb21e9a object returns one.
 * Distinct inputs avoid a false pass from leaving incoming xmm0 untouched. */
#include <string.h>
struct box { __float128 x; };
extern struct box pick(struct box, struct box);
int main(void) {
    struct box a = {1.0}, b = {2.0};
    struct box result = pick(a, b);
    return memcmp(&result, &b, sizeof(result)) != 0;
}
