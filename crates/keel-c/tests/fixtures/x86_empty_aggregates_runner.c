#include <stdarg.h>
struct E {};
union U {};
struct Big { unsigned long words[5]; };
extern unsigned long mix(unsigned long,struct E,unsigned long,struct E,unsigned long,
                         unsigned long,unsigned long,unsigned long,unsigned long,
                         struct E,unsigned long);
extern unsigned long call_mix(void),call_variadic(void),use_empty(unsigned long),inline_call(unsigned long);
extern unsigned long indirect(unsigned long (*)(unsigned long,struct E,unsigned long),struct E,unsigned long,unsigned long);
extern struct E empty(unsigned long,struct E,unsigned long),forward(unsigned long);
extern unsigned long param_address(struct E,unsigned long),union_arg(union U,unsigned long);
extern union U empty_union(union U);
extern struct Big big(struct E,unsigned long);
extern unsigned long variadic(unsigned long,struct E,...);
extern struct E return_with_cleanup(void);
static unsigned effects, empties, cleanups, cleanup_effects;
unsigned long observe_empty(struct E *p) { return p ? 0 : 1000; }
struct E cleanup_empty(struct E *p) {
    cleanups += p ? 1 : 1000; cleanup_effects=effects; return *p;
}
static unsigned long last_a,last_b;
struct E external_effect(void) { ++effects; return (struct E){}; }
struct E external_empty(unsigned long a,struct E e,unsigned long b) {
    ++empties; last_a=a; last_b=b; return e;
}
unsigned long external_mix(unsigned long a,struct E e,unsigned long b,struct E f,
                            unsigned long c,unsigned long d,unsigned long g,
                            unsigned long h,unsigned long i,struct E j,unsigned long k) {
    return a+3*b+5*c+7*d+11*g+13*h+17*i+19*k;
}
static unsigned long target(unsigned long a,struct E e,unsigned long b) { return 3*a+7*b; }
unsigned long external_variadic(unsigned long n,struct E e,...) {
    va_list ap; va_start(ap,e); unsigned long result=0;
    for(unsigned long i=0;i<n;++i) result+=(i+1)*va_arg(ap,unsigned long);
    va_end(ap); return result;
}
int main(void) {
    struct E e={}; union U u={};
    if(mix(1,e,2,e,3,4,5,6,7,e,8)!=454) return 1;
    effects=0;
    if(call_mix()!=454 || effects!=3) return 2;
    if(indirect(target,e,13,17)!=158) return 3;
    effects=empties=0;
    e=empty(13,e,17);
    if(effects!=1 || empties!=1 || last_a!=13 || last_b!=17) return 4;
    effects=empties=0;
    e=forward(23);
    if(effects!=2 || empties!=1 || last_a!=23 || last_b!=24) return 5;
    effects=empties=0;
    if(use_empty(31)!=31 || effects!=2 || empties!=2 || last_a!=31 || last_b!=9) return 6;
    effects=empties=0;
    if(use_empty(0)!=0 || effects!=2 || empties!=1 || last_b!=9) return 7;
    if(param_address(e,37)!=37 || union_arg(empty_union(u),41)!=64) return 8;
    struct Big b=big(e,43);
    for(unsigned i=0;i<5;++i) if(b.words[i]!=43+i) return 9;
    if(variadic(8,e,1UL,2UL,3UL,4UL,5UL,6UL,7UL,8UL)!=204) return 10;
    effects=0;
    if(call_variadic()!=204 || effects!=1) return 11;
    effects=0;
    if(inline_call(47)!=47 || effects!=2) return 12;
    effects=cleanups=0;
    e=return_with_cleanup();
    if(effects!=1 || cleanups!=1 || cleanup_effects!=1) return 13;
    return 0;
}
