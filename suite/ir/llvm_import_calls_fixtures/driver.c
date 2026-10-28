#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
typedef int64_t (*wide_fn)(int64_t,int64_t);
extern int64_t local_xor(int64_t,int64_t), invoke(wide_fn,int64_t,int64_t), spill(wide_fn,int64_t,int64_t);
extern int64_t choose_call(wide_fn,wide_fn,int,int64_t,int64_t);
extern int32_t narrow_many(int32_t (*)(signed char,unsigned short,int32_t,int64_t,signed char,unsigned short,int32_t,int64_t,signed char,unsigned short),int64_t);
static int32_t narrow_callback(signed char a,unsigned short b,int32_t c,int64_t d,signed char e,unsigned short f,int32_t g,int64_t h,signed char i,unsigned short j) {
 return a+2*b+3*c+4*d+5*e+6*f+7*g+8*h+9*i+10*j;
}
extern wide_fn address(int);
extern int64_t phi_call(wide_fn,int,int64_t,int64_t),address_stack(int64_t,int64_t);
extern signed char invoke_signed(signed char (*)(signed char),signed char);
extern unsigned short invoke_unsigned(unsigned short (*)(unsigned short),unsigned short);
extern void invoke_void(void (*)(int64_t*),int64_t*);
extern int64_t *invoke_pointer(int64_t *(*)(int64_t*),int64_t*);
extern int64_t many(int64_t (*)(int64_t,int64_t,int64_t,int64_t,int64_t,int64_t,int64_t,int64_t,int64_t,int64_t),int64_t);
typedef struct { int64_t a,b,c; } Triple;
extern int64_t consume_triple(int64_t),consume_indirect_triple(Triple (*)(int64_t),int64_t);
extern Triple make_triple(int64_t),forward_triple(int64_t),indirect_triple(Triple (*)(int64_t),int64_t);
int64_t external_add(int64_t a,int64_t b) { return a+b; }
static signed char neg(signed char x) { return (signed char)(x ^ 0x55); }
static unsigned short bits(unsigned short x) { return (unsigned short)(x ^ 0x8912); }
static void mutate(int64_t *p) { *p ^= 0x1234; }
static int64_t *pointer(int64_t *p) { return p+1; }
static int64_t ten(int64_t a,int64_t b,int64_t c,int64_t d,int64_t e,int64_t f,int64_t g,int64_t h,int64_t i,int64_t j) {
 return a+b*2+c*3+d*4+e*5+f*6+g*7+h*8+i*9+j*10;
}
Triple external_triple(int64_t x) { Triple t={x+3,x^0x5678,x-11}; return t; }
#define CHECK(x) do { if (!(x)) { fprintf(stderr,"assertion line %d\n",__LINE__); abort(); } } while(0)
int main(void) {
 CHECK(address(1)==external_add); CHECK(address(0)==local_xor);
 for (int64_t x=-1024;x<=1024;x++) {
   int64_t y=x*13+7;
   CHECK(phi_call(external_add,0,x,y)==x+y); CHECK(phi_call(external_add,1,x,y)==(x^y));
   CHECK(address_stack(x,y)==x+y);
   CHECK(narrow_many(narrow_callback,x)==(signed char)x+2*(unsigned short)x+7*x-35+6*65530-133-184-909+10*60001);
   CHECK(invoke(external_add,x,y)==x+y); CHECK(invoke(local_xor,x,y)==(x^y));
   CHECK(spill(external_add,x,y)==x+y); CHECK(spill(local_xor,x,y)==(x^y));
   CHECK(choose_call(external_add,local_xor,1,x,y)==x+y);
   CHECK(choose_call(external_add,local_xor,0,x,y)==(x^y));
   CHECK(invoke_signed(neg,(signed char)x)==neg((signed char)x));
   CHECK(invoke_unsigned(bits,(unsigned short)x)==bits((unsigned short)x));
   int64_t v[2]={x,y}; invoke_void(mutate,v); CHECK(v[0]==(x^0x1234));
   CHECK(invoke_pointer(pointer,v)==v+1); CHECK(many(ten,x)==x+384);
   Triple a=make_triple(x), b=forward_triple(x), c=indirect_triple(external_triple,x), d=indirect_triple(make_triple,x);
   CHECK(consume_triple(x)==((x+3)^(x^0x5678)^(x-11)));
   CHECK(consume_indirect_triple(external_triple,x)==((x+3)^(x^0x5678)^(x-11)));
   CHECK(consume_indirect_triple(make_triple,x)==(x^(x^0x1234)^(x+17)));
   CHECK(a.a==x && a.b==(x^0x1234) && a.c==x+17);
   CHECK(b.a==x+3 && b.b==(x^0x5678) && b.c==x-11);
   CHECK(c.a==b.a && c.b==b.b && c.c==b.c);
   CHECK(d.a==a.a && d.b==a.b && d.c==a.c);
 }
 puts("calls: 2049 independently asserted inputs, mixed objects and aggregate x8");
}
