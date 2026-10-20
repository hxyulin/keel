#if __has_attribute(keel_noninterposable)
#define BIND __attribute__((keel_noninterposable))
#else
#define BIND
#endif
typedef signed char i8;
typedef unsigned char u8;
typedef short i16;
typedef unsigned short u16;
typedef long i64;
typedef unsigned long u64;
BIND i8 clamp_i8(i16 v) { return (i8)(v < -128 ? -128 : v > 127 ? 127 : v); }
BIND u16 mix_u16(u8 a, i8 b, u16 c) { return (u16)(a * 3 + b * 5 + (c >> 2)); }
i64 k_narrow(const i16 *p, u64 n) {
    i64 s=0;
    for(u64 i=0;i<n;++i) {
        i16 v=p[i];
        i8 c=clamp_i8(v);
        u16 m=mix_u16((u8)v,(i8)(v>>8),(u16)v);
        s+=c*7+m-(i16)(v*3)/7+(u8)(v>>3);
    }
    return s;
}
typedef struct { u64 a,b,c,d,e; } Big;
BIND Big adjust(Big p, Big *original) {
    original->a=99;
    p.b+=p.a;
    p.c^=p.e;
    return p;
}
u64 aggregate(u64 x) {
    Big p={x,7,x+11,13,17};
    Big r=adjust(p,&p);
    return r.a+3*r.b+5*r.c+7*p.a+p.b;
}
BIND u64 wrapping(u64 x, u8 y) { return x*0xfedcba9876543211ul+y; }
u64 wrap_call(u64 x,u8 y) { return wrapping(x,y); }
BIND u64 effects(u64 *p,u64 *q,volatile unsigned *v,_Atomic unsigned *a) {
    *p+=1;
    *q+=*p;
    *v+=1;
    (*a)++;
    return *p+*q+*v+*a;
}
u64 effects_call(u64 *p,u64 *q,volatile unsigned *v,_Atomic unsigned *a) { return effects(p,q,v,a); }
extern void capture(u64 *);
BIND u64 captured(u64 x) { u64 local=x; capture(&local); return local; }
u64 capture_call(u64 x) { return captured(x); }
extern volatile unsigned *trace;
BIND int ordered_fault(volatile int *p,int x,int d) {
    *trace=1;
    int y=*p;
    *trace=2;
    int z=x/d;
    *trace=3;
    return y+z;
}
int fault_call(volatile int *p,int x,int d) { return ordered_fault(p,x,d); }
__attribute__((noinline)) BIND int stay(int x) { return x+1; }
int stay_call(int x) { return stay(x); }
BIND int classify(int v) {
    switch(v) {
    case -1000: return 1;
    case -7: return 2;
    case 0: return 3;
    case 3: return 4;
    case 64: return 5;
    case 1048576: return 6;
    case 2147483647: return 7;
    default: return v & 1 ? -1 : -2;
    }
}
int classify_call(int v) { return classify(v); }
