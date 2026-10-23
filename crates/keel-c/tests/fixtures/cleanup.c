extern int printf(const char *, ...);
long trace;
int calls;
int *escaped;
void record(int *p) { trace=trace*10+*p; ++calls; *p=99; }
void plain(void *p) { record(p); }
int returning(int *p) { record(p); return 77; }
void observe(int *p) { trace=trace*10+*escaped; *p=55; }
void mutate_pointer(int **p) { **p=42; }

void normal(void) {
    int a __attribute__((cleanup(record)))=1;
    { int b __attribute__((cleanup(record)))=2; int c __attribute__((cleanup(record)))=3; }
    int d __attribute__((cleanup(returning)))=4;
}
int early(int which) {
    int a __attribute__((cleanup(record)))=1;
    if(which==0) return a;
    { int b __attribute__((cleanup(record)))=2; if(which==1) return b; }
    int c __attribute__((cleanup(record)))=3;
    if(which==2) return c;
    return a+c;
}
void falling(void) { int unused __attribute__((cleanup(record)))=5; }
void void_return(void) { int unused __attribute__((cleanup(record)))=6; return; }
int address(void) {
    int a __attribute__((cleanup(plain)))=7;
    escaped=&a;
    int unused __attribute__((cleanup(observe)))=8;
    return *escaped;
}
void loops(void) {
    for(int i __attribute__((cleanup(record)))=1; i<5; ++i) {
        int x __attribute__((cleanup(record)))=i;
        if(i==2) continue;
        { int y __attribute__((cleanup(record)))=i+1; if(i==3) break; }
    }
    int i=0;
    while(++i<4) { int x __attribute__((cleanup(record)))=i; if(i==1) continue; if(i==2) break; }
    i=0;
    do { int x __attribute__((cleanup(record)))=++i; if(i==1) continue; } while(i<2);
    for(int j __attribute__((cleanup(record)))=9; 0;) { }
}
int for_return(void) {
    for(int i __attribute__((cleanup(record)))=8; i; --i) { int b __attribute__((cleanup(record)))=2; return i+b; }
    return 0;
}
int expressions(void) {
    int x=({ int a __attribute__((cleanup(record)))=2; a; });
    int y=({ int b __attribute__((cleanup(record)))=3; b; });
    ({ int discarded __attribute__((cleanup(record)))=4; });
    for(int i=0; ({ int c __attribute__((cleanup(record)))=5; i<2; });
        ({ int d __attribute__((cleanup(record)))=6; ++i; })) {
        int body __attribute__((cleanup(record)))=7;
    }
    return x+y;
}
struct Small { int x,y; };
struct Large { long x,y,z,w; };
void small_cleanup(struct Small *p) { p->x=99; p->y=98; ++calls; }
void large_cleanup(struct Large *p) { p->x=99; p->w=98; ++calls; }
void complex_cleanup(double _Complex *p) { *p=__builtin_complex(99.0,98.0); ++calls; }
void float_complex_cleanup(float _Complex *p) { *p=__builtin_complex(99.0f,98.0f); ++calls; }
struct Small small_return(void) { struct Small s __attribute__((cleanup(small_cleanup)))={11,12}; return s; }
struct Large large_return(void) { struct Large s __attribute__((cleanup(large_cleanup)))={13,14,15,16}; return s; }
double _Complex complex_return(void) {
    double _Complex z __attribute__((cleanup(complex_cleanup)))=__builtin_complex(17.0,18.0); return z;
}
float _Complex float_complex_return(void) {
    float _Complex z __attribute__((cleanup(float_complex_cleanup)))=__builtin_complex(19.0f,20.0f); return z;
}
int aggregates(void) {
    struct Small a=small_return(); struct Large b=large_return();
    double _Complex c=complex_return(); float _Complex d=float_complex_return();
    struct Small e=({ struct Small x __attribute__((cleanup(small_cleanup)))={21,22}; x; });
    double _Complex f=({ double _Complex z __attribute__((cleanup(complex_cleanup)))=__builtin_complex(23.0,24.0); z; });
    int original=31;
    int *pointer __attribute__((cleanup(mutate_pointer)))=&original;
    // Return evaluation also freezes a read reached through an escaped address.
    return a.x+a.y+b.x+b.y+b.z+b.w+__real__ c+__imag__ c+
        __real__ d+__imag__ d+e.x+e.y+__real__ f+__imag__ f+original;
}
int *live;
void before_vla(int *p) { trace=trace*10+*p; }
void with_vla(int *p) { trace=trace*10+*p+*live; *live=3; }
void array_cleanup(void *p) { int *a=p; trace=trace*10+a[0]; a[1]=7; }
int vlas(int n) {
    int before __attribute__((cleanup(before_vla)))=1;
    int a[n] __attribute__((cleanup(array_cleanup))); a[0]=2; a[1]=8; live=a;
    int after __attribute__((cleanup(with_vla)))=4;
    { int b[n+1]; b[0]=1; int nested __attribute__((cleanup(record)))=5; }
    return a[1];
}
long stress(int n) {
    long sum=0;
    for(int i=0;i<20000;++i) {
        char a[n]; a[n-1]=1;
        int x __attribute__((cleanup(before_vla)))=1;
        // Keep the trace bounded during restoration stress.
        trace=0;
        if(i%2) continue;
        sum+=a[n-1];
    }
    return sum;
}
// The callback's result is discarded through the ordinary aggregate ABI.
struct Small small_callback(int *p) { record(p); return (struct Small){1,2}; }
double _Complex pair_callback(int *p) { record(p); return __builtin_complex(1.0,2.0); }
void callback_results(void) { int x __attribute__((cleanup(small_callback)))=1; int y __attribute__((cleanup(pair_callback)))=2; }
void initialize_only(int *p) { *p=88; ++calls; }
int interrupted_initializer(int flag) {
    int a __attribute__((cleanup(record)))=1;
    int b __attribute__((cleanup(record)))=({ if(flag) return 7; 3; });
    return b;
}
void declarations(void) {
    __attribute__((__cleanup__(record))) int a=1,b=2;
    { int a __attribute__((cleanup(record)))=3; }
    int uninitialized __attribute__((cleanup(initialize_only)));
    int value=4;
    int * __attribute__((cleanup(mutate_pointer))) p=&value;
}
void nested_loops(void) {
    for(int i=1;i<3;++i) {
        int outer __attribute__((cleanup(record)))=i;
        for(int j=1;j<4;++j) {
            int inner __attribute__((cleanup(record)))=j;
            if(j==1) continue;
            break;
        }
    }
}
void fixed_array(int (*p)[3]) { trace=trace*10+(*p)[0]; }
void array_signature(void) { int a[3] __attribute__((cleanup(fixed_array)))={4,5,6}; }
struct Small vla_struct(int n) {
    struct Small a[n] __attribute__((cleanup(array_cleanup)));
    a[0]=(struct Small){11,12};
    return a[0];
}
// A callback can use another object while its VLA lifetime is still active.
// Same-scope restores must interleave with callbacks, and an inner restore
// must preserve an outer allocation across continue and return.
void read_live(int **p) { trace=trace*10+**p; }
int vla_lifetimes(int n, int early) {
    int a[n]; a[0]=2;
    int *outer __attribute__((cleanup(read_live)))=a;
    int b[n+1]; b[0]=3;
    int *inner __attribute__((cleanup(read_live)))=b;
    for(int i=0;i<3;++i) {
        int c[n+2]; c[0]=4+i;
        int *nested __attribute__((cleanup(read_live)))=c;
        if(i==0) continue;
        if(early) return a[0]+b[0]+c[0];
        break;
    }
    return a[0]+b[0];
}
struct Small volatile_return(void) {
    volatile struct Small s __attribute__((cleanup(small_cleanup)))={25,26};
    return s;
}
int volatile_snapshots(void) {
    struct Small a=volatile_return();
    struct Small b=({ volatile struct Small s __attribute__((cleanup(small_cleanup)))={27,28}; s; });
    volatile int scalar __attribute__((cleanup(record)))=7;
    return a.x+a.y+b.x+b.y+scalar;
}
void weak_cleanup(int *p) __attribute__((weak));
void weak_cleanup(int *p) { record(p); }
void late_weak_cleanup(int *p) { record(p); }
void weak_callbacks(void) {
    int x __attribute__((cleanup(weak_cleanup)))=1;
    int y __attribute__((cleanup(late_weak_cleanup)))=2;
}
void late_weak_cleanup(int*) __attribute__((weak));
int main(void) {
    trace=0; calls=0; normal(); printf("normal %ld %d\n",trace,calls);
    for(int i=0;i<4;++i) { trace=0; int v=early(i); printf("early %d %ld\n",v,trace); }
    trace=0; falling(); void_return(); printf("void %ld\n",trace);
    trace=0; int v=address(); printf("address %d %ld\n",v,trace);
    trace=0; loops(); printf("loops %ld\n",trace);
    trace=0; v=for_return(); printf("for %d %ld\n",v,trace);
    trace=0; v=expressions(); printf("expr %d %ld\n",v,trace);
    calls=0; v=aggregates(); printf("aggregates %d %d\n",v,calls);
    trace=0; v=vlas(4); printf("vla %d %ld\n",v,trace);
    trace=0; callback_results(); printf("results %ld\n",trace);
    trace=0; calls=0; declarations(); printf("decl %ld %d\n",trace,calls);
    trace=0; v=interrupted_initializer(1); printf("init1 %d %ld\n",v,trace);
    trace=0; v=interrupted_initializer(0); printf("init0 %d %ld\n",v,trace);
    trace=0; nested_loops(); printf("nested %ld\n",trace);
    trace=0; array_signature(); printf("array %ld\n",trace);
    trace=0; struct Small vs=vla_struct(3); printf("vla-struct %d %d %ld\n",vs.x,vs.y,trace);
    trace=0; v=vla_lifetimes(32,0); printf("lifetimes %d %ld\n",v,trace);
    trace=0; v=vla_lifetimes(32,1); printf("lifetimes-early %d %ld\n",v,trace);
    trace=0; v=volatile_snapshots(); printf("volatile %d %ld\n",v,trace);
    trace=0; weak_callbacks(); printf("weak %ld\n",trace);
    long s=stress(4096); printf("stress %ld %ld\n",s,trace);
    return 0;
}
