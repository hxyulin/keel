#include <stdint.h>
typedef int64_t (*wide_fn)(int64_t, int64_t);
typedef signed char (*signed_fn)(signed char);
typedef unsigned short (*unsigned_fn)(unsigned short);
extern int64_t external_add(int64_t, int64_t);
int64_t local_xor(int64_t a, int64_t b) { return a ^ b; }
wide_fn address(int choose) { return choose ? external_add : local_xor; }
int64_t invoke(wide_fn f, int64_t a, int64_t b) { return f(a, b); }
int64_t spill(wide_fn f, int64_t a, int64_t b) {
    wide_fn volatile saved = f;
    return saved(a, b);
}
int64_t choose_call(wide_fn a, wide_fn b, int choose, int64_t x, int64_t y) {
    wide_fn f;
    if (choose) f = a; else f = b;
    return f(x, y);
}
signed char invoke_signed(signed_fn f, signed char x) { return f(x); }
unsigned short invoke_unsigned(unsigned_fn f, unsigned short x) { return f(x); }
void invoke_void(void (*f)(int64_t *), int64_t *p) { f(p); }
int64_t *invoke_pointer(int64_t *(*f)(int64_t *), int64_t *p) { return f(p); }
int64_t many(int64_t (*f)(int64_t,int64_t,int64_t,int64_t,int64_t,int64_t,int64_t,int64_t,int64_t,int64_t), int64_t x) {
    return f(x,2,3,4,5,6,7,8,9,10);
}
int32_t narrow_many(int32_t (*f)(signed char,unsigned short,int32_t,int64_t,signed char,unsigned short,int32_t,int64_t,signed char,unsigned short), int64_t x) {
    return f((signed char)x,(unsigned short)x,(int32_t)x,x,-7,65530,-19,-23,-101,60001);
}
