struct E {};
union U {};
struct Big { unsigned long words[5]; };
extern struct E external_effect(void);
extern unsigned long external_mix(unsigned long, struct E, unsigned long, struct E,
                                  unsigned long, unsigned long, unsigned long,
                                  unsigned long, unsigned long, struct E, unsigned long);
extern struct E external_empty(unsigned long, struct E, unsigned long);
unsigned long mix(unsigned long a, struct E e, unsigned long b, struct E f,
                  unsigned long c, unsigned long d, unsigned long g,
                  unsigned long h, unsigned long i, struct E j, unsigned long k) {
    return a + 3*b + 5*c + 7*d + 11*g + 13*h + 17*i + 19*k;
}
unsigned long call_mix(void) {
    return external_mix(1, external_effect(), 2, external_effect(), 3,4,5,6,7,
                        external_effect(),8);
}
unsigned long indirect(unsigned long (*f)(unsigned long,struct E,unsigned long),
                       struct E e, unsigned long a, unsigned long b) {
    return f(a,e,b);
}
struct E empty(unsigned long a, struct E e, unsigned long b) {
    external_empty(a,e,b);
    return external_effect();
}
struct E forward(unsigned long a) { return empty(a,external_effect(),a+1); }
unsigned long use_empty(unsigned long a) {
    struct E e = external_effect(), f;
    f = a ? empty(a,e,7) : external_effect();
    external_empty(a, f, 9);
    return sizeof(e) + sizeof(f) + a;
}
extern unsigned long observe_empty(struct E *);
unsigned long param_address(struct E e, unsigned long n) {
    struct E *p=&e;
    return observe_empty(p) + sizeof(*p) + n;
}
struct Big big(struct E e, unsigned long a) {
    struct Big result={{a,a+1,a+2,a+3,a+4}};
    return result;
}
union U empty_union(union U u) { return u; }
unsigned long union_arg(union U u, unsigned long a) { return a+23; }
unsigned long variadic(unsigned long n, struct E last, ...) {
    __builtin_va_list ap;
    __builtin_va_start(ap,last);
    unsigned long result=0;
    for(unsigned long i=0;i<n;++i) result += (i+1)*__builtin_va_arg(ap,unsigned long);
    __builtin_va_end(ap);
    return result;
}
extern unsigned long external_variadic(unsigned long,struct E,...);
unsigned long call_variadic(void) {
    return external_variadic(8,external_effect(),1UL,2UL,3UL,4UL,5UL,6UL,7UL,8UL);
}
static __attribute__((always_inline)) inline struct E inline_empty(struct E e) {
    external_effect(); return e;
}
unsigned long inline_call(unsigned long n) {
    struct E e=inline_empty(external_effect());
    return sizeof(e)+n;
}

extern struct E cleanup_empty(struct E *);
struct E return_with_cleanup(void) {
    struct E e __attribute__((cleanup(cleanup_empty))) = {};
    return external_effect();
}
