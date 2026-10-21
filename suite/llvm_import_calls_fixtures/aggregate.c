#include <stdint.h>
typedef struct { int64_t a, b, c; } Triple;
extern Triple external_triple(int64_t);
Triple make_triple(int64_t x) { Triple t = {x, x ^ 0x1234, x + 17}; return t; }
Triple forward_triple(int64_t x) { return external_triple(x); }
Triple indirect_triple(Triple (*f)(int64_t), int64_t x) { return f(x); }
int64_t consume_triple(int64_t x) {
    Triple t = external_triple(x);
    return t.a ^ t.b ^ t.c;
}
int64_t consume_indirect_triple(Triple (*f)(int64_t), int64_t x) {
    Triple t = f(x);
    return t.a ^ t.b ^ t.c;
}
