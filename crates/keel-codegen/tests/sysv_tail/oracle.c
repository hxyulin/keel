/* Independent C SysV boundaries for corpus.ir. No generated Keel layout data. */
typedef unsigned long long U;
typedef unsigned char B;
typedef signed char I8 __attribute__((vector_size(8)));
typedef short I16 __attribute__((vector_size(8)));
typedef int I32 __attribute__((vector_size(8)));
typedef long long I64 __attribute__((vector_size(8)));
typedef float F32 __attribute__((vector_size(8)));
typedef int Q __attribute__((vector_size(16)));
typedef int Y __attribute__((vector_size(32)));
extern int printf(const char *, ...);
static unsigned failures;
static void check(U actual, U expected, const char *name) {
    if (actual != expected) {
        printf("FAIL %s: %llx != %llx\n", name, actual, expected);
        ++failures;
    }
}
static U bytes(const void *p, unsigned n) {
    const B *b = p;
    U h = 0;
    for (unsigned i = 0; i < n; ++i) h = h * 131 + b[i];
    return h;
}
static void fill(void *p, unsigned n, unsigned seed) {
    B *b = p;
    for (unsigned i = 0; i < n; ++i) b[i] = (seed * 7 + i * 11) & 63;
}
#define PAIR(N,A,C) \
    typedef struct { A a; C b; } N; \
    static U hash_##N(N v) { return bytes(&v.a,sizeof v.a)*17+bytes(&v.b,sizeof v.b); } \
    static N make_##N(unsigned seed) { N v = {0}; fill(&v.a,sizeof v.a,seed); fill(&v.b,sizeof v.b,seed+19); return v; }
#define SINGLE(N,A) \
    typedef struct { A a; } N; \
    static U hash_##N(N v) { return bytes(&v.a,sizeof v.a); } \
    static N make_##N(unsigned seed) { N v = {0}; fill(&v.a,sizeof v.a,seed); return v; }
PAIR(gp,U,U)
typedef struct { U a,b,c; } mem;
static U hash_mem(mem v) { return v.a*17+v.b*31+v.c*43; }
static mem make_mem(unsigned s) { return (mem){s+1,s+11,s+101}; }
SINGLE(q,Q)
#if defined(__AVX__)
SINGLE(y,Y)
#endif
PAIR(qm,Q,U)
typedef struct { long double a; } fx;
static U hash_fx(fx v) { return bytes(&v.a,10); } /* Exclude x87 padding. */
static fx make_fx(unsigned s) { return (fx){(long double)s + 0.25L}; }
PAIR(di8_i64_0,I8,U)
PAIR(di8_i64_1,U,I8)
PAIR(di8_f64_0,I8,double)
PAIR(di8_f64_1,double,I8)
PAIR(di16_i64_0,I16,U)
PAIR(di16_i64_1,U,I16)
PAIR(di16_f64_0,I16,double)
PAIR(di16_f64_1,double,I16)
PAIR(di32_i64_0,I32,U)
PAIR(di32_i64_1,U,I32)
PAIR(di32_f64_0,I32,double)
PAIR(di32_f64_1,double,I32)
PAIR(di64_i64_0,I64,U)
PAIR(di64_i64_1,U,I64)
PAIR(di64_f64_0,I64,double)
PAIR(di64_f64_1,double,I64)
PAIR(df32_i64_0,F32,U)
PAIR(df32_i64_1,U,F32)
PAIR(df32_f64_0,F32,double)
PAIR(df32_f64_1,double,F32)
PAIR(d8,F32,U)

#define PARAMS(T) T a0,T a1,T a2,T a3,T a4,T a5,T a6,T a7,T a8,T a9
#define TYPES(T) T,T,T,T,T,T,T,T,T,T
#define FORWARD a[0],a[1],a[2],a[3],a[4],a[5],a[6],a[7],a[8],a[9]
#define FAMILY(N) \
    U c_##N(PARAMS(N)) { \
        N v[10]={a0,a1,a2,a3,a4,a5,a6,a7,a8,a9}; U r=0; \
        for (unsigned i=0;i<10;++i) r+=(i+1)*hash_##N(v[i]); return r; \
    } \
    N id_##N(N a) { return a; } \
    extern U tail_##N##_0_0(TYPES(N)); \
    extern U tail_##N##_0_1(TYPES(N)); \
    extern U tail_##N##_1_0(TYPES(N),U(*)(TYPES(N))); \
    extern U tail_##N##_1_1(TYPES(N),U(*)(TYPES(N))); \
    extern N result_##N(N); \
    extern N result_ind_##N(N,N(*)(N)); \
    extern U only_##N(TYPES(N),U); \
    static void test_##N(unsigned seed) { \
        N a[10]; U reverse=0; \
        for(unsigned i=0;i<10;++i) { a[i]=make_##N(seed+i); reverse+=(10-i)*hash_##N(a[i]); } \
        U repeat=55*hash_##N(a[0]); \
        check(tail_##N##_0_0(FORWARD),reverse,#N " direct reverse"); \
        check(tail_##N##_0_1(FORWARD),repeat,#N " direct repeat"); \
        check(tail_##N##_1_0(FORWARD,c_##N),reverse,#N " indirect reverse"); \
        check(tail_##N##_1_1(FORWARD,c_##N),repeat,#N " indirect repeat"); \
        check(hash_##N(result_##N(a[3])),hash_##N(a[3]),#N " result"); \
        check(hash_##N(result_ind_##N(a[7],id_##N)),hash_##N(a[7]),#N " indirect result"); \
        check(only_##N(FORWARD,seed+991),seed+991,#N " scalar tail"); \
    }
FAMILY(gp)
FAMILY(mem)
FAMILY(q)
#if defined(__AVX__)
FAMILY(y)
#endif
FAMILY(qm)
FAMILY(fx)
FAMILY(di8_i64_0)
FAMILY(di8_i64_1)
FAMILY(di8_f64_0)
FAMILY(di8_f64_1)
FAMILY(di16_i64_0)
FAMILY(di16_i64_1)
FAMILY(di16_f64_0)
FAMILY(di16_f64_1)
FAMILY(di32_i64_0)
FAMILY(di32_i64_1)
FAMILY(di32_f64_0)
FAMILY(di32_f64_1)
FAMILY(di64_i64_0)
FAMILY(di64_i64_1)
FAMILY(di64_f64_0)
FAMILY(di64_f64_1)
FAMILY(df32_i64_0)
FAMILY(df32_i64_1)
FAMILY(df32_f64_0)
FAMILY(df32_f64_1)
FAMILY(d8)
U scalar(U x) { return x; }
#define SCALAR_FAMILY(N,T,H) \
    U scalar_##N(PARAMS(T)) { T v[10]={a0,a1,a2,a3,a4,a5,a6,a7,a8,a9}; U r=0; \
        for(unsigned i=0;i<10;++i) r+=(i+1)*(H); return r; } \
    extern U tail_scalar_##N##_0(TYPES(T)); \
    extern U tail_scalar_##N##_1(TYPES(T),U(*)(TYPES(T))); \
    static void test_scalar_##N(void) { T a[10]; U expected=0; \
        for(unsigned i=0;i<10;++i) { a[i]=(T)(i+3); expected+=(10-i)*(U)a[i]; } \
        check(tail_scalar_##N##_0(FORWARD),expected,#N " scalar direct"); \
        check(tail_scalar_##N##_1(FORWARD,scalar_##N),expected,#N " scalar indirect"); }
SCALAR_FAMILY(int,U,(U)v[i])
SCALAR_FAMILY(fp,double,(U)v[i])
#define FIVE(T) T,T,T,T,T
#define WIDE(x) ((U)(x) ^ (U)((x)>>64))
U scalar_wideint(__int128 a,__int128 b,__int128 c,__int128 d,__int128 e) {
    return WIDE(a)+2*WIDE(b)+3*WIDE(c)+4*WIDE(d)+5*WIDE(e);
}
extern U tail_scalar_wideint_0(FIVE(__int128));
extern U tail_scalar_wideint_1(FIVE(__int128),U(*)(FIVE(__int128)));
U scalar_extended(long double a,long double b,long double c) { return (U)(a+3*b+7*c); }
extern U tail_scalar_extended_0(long double,long double,long double);
extern U tail_scalar_extended_1(long double,long double,long double,U(*)(long double,long double,long double));
#define G6 U a,U b,U c,U d,U e,U f
#define F8 double g,double h,double i,double j,double k,double l,double m,double n
#define GP_SUM (a+3*b+5*c+7*d+11*e)
#define FP7_SUM (13*g+17*h+19*i+23*j+29*k+31*l+37*m)
U pressure(G6,F8,d8 x,d8 y,U z,double w) {
    return GP_SUM+41*f+(U)(FP7_SUM+43*n+47*w)+53*hash_d8(x)+59*hash_d8(y)+61*z;
}
extern U tail_pressure(G6,F8,d8,d8,U,double);
U rollback_gp(G6,double g,double h,double i,double j,double k,double l,double m,d8 x,U z,double w) {
    return GP_SUM+41*f+(U)(FP7_SUM+47*w)+53*hash_d8(x)+61*z;
}
extern U tail_rollback_gp(G6,double,double,double,double,double,double,double,d8,U,double);
U rollback_fp(U a,U b,U c,U d,U e,F8,d8 x,U z,double w) {
    return GP_SUM+(U)(FP7_SUM+43*n+47*w)+53*hash_d8(x)+61*z;
}
extern U tail_rollback_fp(U,U,U,U,U,F8,d8,U,double);
extern U depth_entry(mem,mem);
extern U alias_entry(mem *);
/* overlap's interior source extends beyond its first byval object. Check the
   in-bounds words only; no promise is made about adjacent argument storage. */
static mem overlap_first,overlap_second;
U alias_target(mem a,mem b) {
    return a.a==overlap_second.a && a.b==overlap_second.b && a.c==overlap_second.c
        && b.a==overlap_first.b && b.b==overlap_first.c;
}
extern U overlap(mem,mem);
int main(void) {
    for(unsigned seed=1;seed<=3;++seed) {
#define RUN(N) test_##N(seed);
        RUN(gp) RUN(mem) RUN(q) RUN(qm) RUN(fx)
#if defined(__AVX__)
        RUN(y)
#endif
        RUN(di8_i64_0) RUN(di8_i64_1) RUN(di8_f64_0) RUN(di8_f64_1)
        RUN(di16_i64_0) RUN(di16_i64_1) RUN(di16_f64_0) RUN(di16_f64_1)
        RUN(di32_i64_0) RUN(di32_i64_1) RUN(di32_f64_0) RUN(di32_f64_1)
        RUN(di64_i64_0) RUN(di64_i64_1) RUN(di64_f64_0) RUN(di64_f64_1)
        RUN(df32_i64_0) RUN(df32_i64_1) RUN(df32_f64_0) RUN(df32_f64_1) RUN(d8)
    }
    test_scalar_int(); test_scalar_fp();
    __int128 a=((__int128)19<<80)+3,b=((__int128)71<<92)+5;
    U expected=scalar_wideint(13,11,7,b,a);
    check(tail_scalar_wideint_0(a,b,7,11,13),expected,"i128 direct");
    check(tail_scalar_wideint_1(a,b,7,11,13,scalar_wideint),expected,"i128 indirect");
    check(tail_scalar_extended_0(3,5,11),47,"f80 direct");
    check(tail_scalar_extended_1(3,5,11,scalar_extended),47,"f80 indirect");
    d8 x=make_d8(7),y=make_d8(91);
    check(tail_pressure(1,2,3,4,5,6,7,8,9,10,11,12,13,14,x,y,15,16),
          pressure(1,2,3,4,5,6,7,8,9,10,11,12,13,14,y,x,15,16),"pressure swap");
    check(tail_rollback_gp(1,2,3,4,5,6,7,8,9,10,11,12,13,x,15,16),
          rollback_gp(1,2,3,4,5,6,7,8,9,10,11,12,13,x,15,16),"GP rollback");
    check(tail_rollback_fp(1,2,3,4,5,7,8,9,10,11,12,13,14,x,15,16),
          rollback_fp(1,2,3,4,5,7,8,9,10,11,12,13,14,x,15,16),"FP rollback");
    mem original=make_mem(771),out=original;
    check(alias_entry(&out),1,"sret alias return pointer");
    check(hash_mem(out),hash_mem(original),"sret alias value");
    check(depth_entry(make_mem(3),make_mem(8)),1,"constant tail stack depth");
    overlap_first=make_mem(13); overlap_second=make_mem(81);
    check(overlap(overlap_first,overlap_second),1,"overlapping byval sources");
    if (!failures) printf("KEEL SYSV TAIL PASS\n");
    return failures!=0;
}
