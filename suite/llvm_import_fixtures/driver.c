#include <stdint.h>
#include <stdio.h>
#include <limits.h>
typedef unsigned long long U;
typedef long long S;
extern U mix(U, U), choose(U, U), helper(U, U), callhelper(U, U);
extern S signed_div(S), signed_rem(S), narrow_sign(unsigned char);
extern U memory(volatile U *, U);
static int check(U a, U b, U *checksum) {
    volatile U cell = b;
    if (mix(a,b) != ((a+b)*7 ^ (a>>3))) return 1;
    if (choose(a,b) != (a<b ? a^b : a+b)) return 2;
    if (signed_div((S)a) != (S)a/3) return 3;
    if (signed_rem((S)a) != (S)a%-5) return 4;
    if (narrow_sign((unsigned char)a) != (signed char)a) return 5;
    if (memory(&cell,a) != (a^b) || cell != a) return 6;
    if (helper(a,b) != ((a^b)+19)) return 7;
    if (callhelper(a,b) != (((a^b)+19)^a)) return 8;
    *checksum = (*checksum ^ mix(a,b)) * 1099511628211ULL;
    return 0;
}
int main(void) {
    U edges[] = {0,1,2,3,4,5,127,128,255,256,65535,1ULL<<31,
                 (1ULL<<63)-1,1ULL<<63,(1ULL<<63)+1,ULLONG_MAX-1,ULLONG_MAX};
    U sum = 0, rng = 0x82372616533ULL;
    for (unsigned i=0;i<sizeof(edges)/sizeof(edges[0]);i++)
        for (unsigned j=0;j<sizeof(edges)/sizeof(edges[0]);j++) {
            int e=check(edges[i],edges[j],&sum); if(e) return e;
        }
    for (unsigned i=0;i<20000;i++) {
        rng ^= rng<<13; rng ^= rng>>7; rng ^= rng<<17;
        int e=check(rng, rng*0xdeadbeef12345679ULL, &sum); if(e) return e;
    }
    printf("%llu\n",sum);
}
