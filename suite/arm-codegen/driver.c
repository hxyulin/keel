/* Compiled once by Clang, linked unchanged to every kernel, without LTO. */
#define _POSIX_C_SOURCE 200809L
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>
#include <inttypes.h>
extern unsigned long long kernel(const unsigned long long *, unsigned long);
typedef unsigned long long U;
/* Bit-at-a-time truth-table oracle, independent of the kernel expressions. */
static U bits(U x, U m, int op, int width) {
    U out=0;
    for(int b=0;b<width;b++) {
        unsigned a=(x>>b)&1, c=(m>>b)&1;
        unsigned v=op==0 ? a*c : op==1 ? (a+c!=0) : (a!=c);
        out += (U)v << b;
    }
    return out;
}
static U oracle(const U *p, unsigned long n, int id) {
    U s=0;
    for(unsigned long i=0;i<n;i++) {
        U x=p[i], y=p[n+i];
        switch(id) {
        case 0: return n;
        case 1: s+=bits(x,0x00ff00ff,0,32); break;
        case 2: s+=bits(x,0x80000001,1,32); break;
        case 3: s+=bits(x,0xff00ff00,2,32); break;
        case 4: s+=bits(x,0x00ff00ff00ff00ffULL,0,64); break;
        case 5: s+=bits(x,0x8000000000000001ULL,1,64); break;
        case 6: s+=bits(x,0xff00ff00ff00ff00ULL,2,64); break;
        case 7: s+=bits(x,0x0123456789abcdefULL,0,64); break;
        case 8: s+=bits(x,0x00ff00ff00ff00ffULL,0,64)+bits(y,0x00ff00ff00ff00ffULL,2,64)+bits(x,0x00ff00ff00ff00ffULL,1,64); break;
        case 9: s+=bits(x,0x80000001,0,32)+bits(y,0x8000000000000001ULL,2,64); break;
        case 10: s+=x+20014547599360ULL; break;
        case 11: s+=x-261456134144001ULL; break;
        case 12:
            s+=bits(x,0x00ff00ff00ff00ffULL,0,64)+bits(y,0x8000000000000001ULL,2,64);
            s+=bits(x,0xff00ff00ff00ff00ULL,2,64)+bits(y,0x8000000000000001ULL,1,64);
            s+=x-261456134144001ULL;
            s+=bits(y,0x0123456789abcdefULL,0,64)+bits(x,0x80000001,1,32);
            s+=bits(y,0xff00ff00,2,32)+y+305397760ULL; break;
        default: exit(2);
        }
    }
    return s;
}
static double now(void) {
    struct timespec t;
    if(clock_gettime(CLOCK_MONOTONIC,&t)) exit(5);
    return t.tv_sec + t.tv_nsec*1e-9;
}
int main(int argc,char **argv) {
    if(argc!=6 && argc!=7) return 2;
    int id=atoi(argv[1]);
    unsigned long n=strtoul(argv[2],0,10), repeat=strtoul(argv[3],0,10);
    U seed=strtoull(argv[4],0,10), state=seed, sum=0;
    int poison=argc==7; /* Test-only intentional oracle mismatch. */
    if(id<0 || id>12 || n>8192 || !repeat || repeat>1000000000UL) return 2;
    U *p=malloc((2*n+1)*sizeof(U)); if(!p) return 2;
    for(unsigned long i=0;i<2*n+1;i++) {
        state=state*6364136223846793005ULL+1442695040888963407ULL;
        p[i]=state;
        if(argv[5][0]=='e') p[i]=(i%4==0)?0:(i%4==1)?~0ULL:(i%4==2)?0x8000000000000000ULL:1;
    }
    U want=oracle(p,n,id);
    if(kernel(p,n)!=want) {fprintf(stderr,"preflight checksum mismatch\n");return 3;}
    double start=now();
    for(unsigned long r=0;r<repeat;r++) sum+=kernel(p,n);
    double seconds=now()-start;
    if(sum!=want*(U)repeat+(U)poison) {fprintf(stderr,"timed checksum mismatch\n");return 4;}
    if(!(seconds>0)) return 5;
    printf("{\"seconds\":%.9f,\"checksum\":\"%016llx\",\"single\":\"%016llx\",\"n\":%lu,\"repeat\":%lu,\"seed\":\"%llu\"}\n",seconds,sum,want,n,repeat,seed);
    free(p); return 0;
}
