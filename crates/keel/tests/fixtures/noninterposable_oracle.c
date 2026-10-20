#include <stdint.h>
#include <limits.h>
#include <assert.h>
#include <signal.h>
#include <sys/mman.h>
#include <sys/wait.h>
#include <unistd.h>
typedef struct { uint64_t a,b,c,d,e; } Big;
extern signed char clamp_i8(short);
extern unsigned short mix_u16(unsigned char,signed char,unsigned short);
extern long k_narrow(const short*,unsigned long);
extern Big adjust(Big,Big*);
extern unsigned long aggregate(unsigned long),wrapping(unsigned long,unsigned char),wrap_call(unsigned long,unsigned char);
extern unsigned long effects(unsigned long*,unsigned long*,volatile unsigned*,_Atomic unsigned*);
extern unsigned long effects_call(unsigned long*,unsigned long*,volatile unsigned*,_Atomic unsigned*);
extern unsigned long captured(unsigned long),capture_call(unsigned long);
extern int ordered_fault(volatile int*,int,int),fault_call(volatile int*,int,int),stay(int),stay_call(int);
extern int classify(int),classify_call(int);
static int expected_class(int v) {
    const int cases[]={-1000,-7,0,3,64,1048576,INT_MAX};
    for(unsigned i=0;i<7;i++) if(v==cases[i]) return (int)i+1;
    return (v&1)?-1:-2;
}
volatile unsigned *trace;
static unsigned captures;
void capture(unsigned long *p) { *p+=37; captures++; }
static long element(short v) {
    int c=v < -128 ? -128 : v > 127 ? 127 : v;
    unsigned short m=(unsigned char)v*3+(signed char)(v>>8)*5+((unsigned short)v>>2);
    return c*7+m-(short)(v*3)/7+(unsigned char)(v>>3);
}
int main(int argc,char **argv) {
    trace=mmap(0,sizeof(*trace),PROT_READ|PROT_WRITE,MAP_SHARED|MAP_ANON,-1,0);
    assert(trace!=MAP_FAILED);
    if(argc>1) {
        // Defined Keel IR division traps; do not execute C UB in the Clang oracle.
        for(int mode=0;mode<3;mode++) {
            *trace=0;
            pid_t p=fork(); assert(p>=0);
            if(!p) { int y=9; fault_call(mode==0?0:&y,mode==2?INT_MIN:9,mode==2?-1:0); _exit(77); }
            int status;assert(waitpid(p,&status,0)==p);assert(WIFSIGNALED(status));
#ifdef __APPLE__
            int divide_signal=SIGTRAP;
#else
            int divide_signal=SIGILL;
#endif
            assert(WTERMSIG(status)==(mode==0?SIGSEGV:divide_signal));
            assert(*trace==(mode==0?1:2));
        }
        return 0;
    }
    static short input[65536];
    long total=0;
    for(int i=0;i<65536;i++) {
        short v=(short)i;input[i]=v;
        int expected=v < -128 ? -128 : v > 127 ? 127 : v;
        assert(clamp_i8(v)==expected);
        assert(mix_u16((unsigned char)v,(signed char)(v>>8),(unsigned short)v)==(unsigned short)((unsigned char)v*3+(signed char)(v>>8)*5+((unsigned short)v>>2)));
        total+=element(v);
    }
    assert(k_narrow(input,65536)==total);
    unsigned sizes[]={0,1,2,7,8,9,15,16,17,31,32,33,127,255};
    for(unsigned j=0;j<sizeof(sizes)/sizeof(*sizes);j++) {
        long sum=0;for(unsigned i=0;i<sizes[j];i++)sum+=element(input[i]);
        assert(k_narrow(input,sizes[j])==sum);
    }
    assert(k_narrow(0,0)==0);
    const int cases[]={INT_MIN,INT_MAX,-1000,-7,0,3,64,1048576};
    for(unsigned i=0;i<sizeof(cases)/sizeof(*cases);i++) {
        for(int delta=-1;delta<=1;delta++) {
            int64_t wide=(int64_t)cases[i]+delta;
            if(wide<INT_MIN||wide>INT_MAX) continue;
            int v=(int)wide;
            assert(classify(v)==expected_class(v));
            assert(classify_call(v)==expected_class(v));
        }
    }
    uint32_t state=1;
    for(unsigned i=0;i<100000;i++) {
        state=state*1664525u+1013904223u;
        int v=(int32_t)state;
        assert(classify_call(v)==expected_class(v));
    }
    uint64_t values[]={0,1,127,128,255,65535,1ul<<63,~0ul};
    for(unsigned i=0;i<8;i++) {
        uint64_t x=values[i];
        assert(wrapping(x,(unsigned char)x)==x*0xfedcba9876543211ul+(unsigned char)x);
        assert(wrap_call(x,(unsigned char)x)==x*0xfedcba9876543211ul+(unsigned char)x);
        assert(aggregate(x)==x+3*(7+x)+5*((x+11)^17)+7*99+7);
        Big p={x,7,x+11,13,17};Big r=adjust(p,&p);
        assert(r.a==x&&r.b==7+x&&r.c==((x+11)^17)&&r.d==13&&r.e==17&&p.a==99&&p.b==7);
        assert(captured(x)==x+37);assert(capture_call(x)==x+37);
    }
    assert(captures==16);
    for(int same=0;same<2;same++)for(int direct=0;direct<2;direct++) {
        unsigned long p=~0ul,q=17;volatile unsigned v=~0u;_Atomic unsigned a=~0u;
        unsigned long result=direct?effects(&p,same?&p:&q,&v,&a):effects_call(&p,same?&p:&q,&v,&a);
        assert(p==0&&q==17&&v==0&&a==0&&result==(same?0:17));
    }
    int y=9;assert(fault_call(&y,19,3)==15&&*trace==3);
    assert(ordered_fault(&y,-19,3)==3&&*trace==3);
    assert(stay(10)==11&&stay_call(-10)==-9);
    return 0;
}
