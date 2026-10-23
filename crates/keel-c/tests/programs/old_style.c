int printf(const char *, ...);
int forward();
int before(void) { return forward((char)-7, 1.25f); }
int forward(c, f) float f; char c; { return c + (int)(f * 4); }
int forward(int, double);
int promoted(int, int, int, int, double);
int promoted(c, u, s, b, f)
    volatile char c;
    unsigned short u;
    short s;
    _Bool b;
    float f;
{
    char *pc = (char *)&c;
    unsigned short *pu = &u;
    short *ps = &s;
    _Bool *pb = &b;
    float *pf = &f;
    *pc += 2; *pu -= 3; *ps += 4; *pb = !*pb; *pf += 0.5f;
    return c + u + s + b + (int)(f * 8)
        + sizeof c + 10*sizeof u + 100*sizeof s + 1000*sizeof b + 10000*sizeof f;
}
int add(int x) { return x+3; }
int adjusted(p, cb, a, matrix)
    int matrix[][2];
    int a[];
    int cb(int);
    int *restrict p;
{ *p += 1; a[1] += 2; return cb(*p) + a[1] + matrix[1][1]; }
int (*factory(x))(int) int x; { return x ? add : 0; }
int indirect();
int indirect(x) int x; { return x*3; }
int shadow=8;
typedef short Small;
int scoped(shadow, x) Small x; int shadow;
{ { int shadow=3; x+=shadow; } return shadow+x; }
int attributed(x) __attribute__((noinline,nothrow)) int x __attribute__((unused));
{ return x+2; }
int ssa(int,int,int,double);
int ssa(c,s,b,f) char c; short s; _Bool b; float f;
{ return c+10*s+100*b+1000*(f==16777216.0f); }
int slots(int,int,int,double);
int slots(c,s,b,f) char c; short s; _Bool b; float f;
{ char *pc=&c; short *ps=&s; _Bool *pb=&b; float *pf=&f;
  return *pc+10**ps+100**pb+1000*(*pf==16777216.0f); }
int array_value(int *a) { return a[0]; }
int nested(n,cb) int n; int cb(int a[n++]);
{ int a[1]={n}; return n+cb(a); }
long width;
int ordered(width,a) int a[2][sizeof width]; char width;
{ return sizeof *a+width; }
unsigned long unsigned_entry(a,b,c)
    unsigned int c; unsigned short b; unsigned char a;
{ return (unsigned long)a + 1000UL*b + c; }
int prior(int, double);
int prior_use(void) { return prior(511, 16777217.0); }
int prior(c,f) unsigned char c; float f;
{ return c + 1000*(f == 16777216.0f); }
int later();
int later_use(void) { return later((unsigned short)65535, 16777217.0); }
int later(c,f) unsigned short c; float f;
{ return c + 1000*(f == 16777216.0f); }
int later(int,double);
int eval_once(x,f) unsigned char x; float f; { return x+(int)f; }
typedef int Vec __attribute__((vector_size(16)));
static int cleaned;
void tidy(int *p) { cleaned += *p; }
__attribute__((weak)) int weak_value(int x) { return x+2; }
int modern_vm(int n, int a[n++]) { return n+a[0]; }
int combined(n,f) short n; float f;
{
    int a[n]; a[0]=n;
    int saved __attribute__((cleanup(tidy)))=3;
    _Atomic int atom=4; atom++;
    double _Complex z=f+n*(__extension__ 1.0i);
    z *= 0x1.8p+1+(__extension__ 0x1p+0i);
    Vec v={n,2,3,4}; Vec w=v+v;
    return modern_vm(n,a)+atom+weak_value(w[0])
        +(int)(__real__ z+__imag__ z)+saved;
}
int main(void) {
    int n=4, a[2]={1,2}, matrix[2][2]={{1,2},{3,4}}, wide[2][8], ticks=0;
    int (*fp)()=indirect;
    int (*pp)(int,int,int,int,double)=promoted;
    printf("%d %d %d %d %d %d %d %d %d\n", before(),
        promoted((char)-12,(unsigned short)65530,(short)-300,(_Bool)1,1.25f),
        pp(-12,65530,-300,1,1.25), adjusted(&n,add,a,matrix),
        fp(++ticks), scoped(10,5), factory(1)(9), attributed(2), shadow);
    printf("%d %d %d %d\n", n,a[1],ticks,forward((char)-2,2.5f));
    printf("%d %d %d %d\n", ssa(257,65535,1,16777217.0), slots(257,65535,1,16777217.0),
        nested(2,array_value), ordered(2,wide));
    {
        unsigned long (*up)(int,int,unsigned int)=unsigned_entry;
        int (*lp)()=later;
        int counter=0;
        printf("%lu %lu %d %d %d %d\n",
            unsigned_entry((unsigned char)255,(unsigned short)65535,4294967295U),
            up(255,65535,4294967295U), prior_use(), later_use(),
            lp((unsigned short)65535,16777217.0), eval_once(++counter, 2.5f));
        printf("%d\n", counter);
    }
    printf("%d\n", combined(3,2.5f));
    printf("%d\n", cleaned);
    return 0;
}
