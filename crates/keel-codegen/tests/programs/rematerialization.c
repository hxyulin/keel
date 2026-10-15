#include <stdint.h>
#include <stdio.h>
#include "clobber.h"
int64_t remat_probe(int64_t, int64_t);
__int128 remat_shift(__int128);
int64_t remat_sum(int64_t a,int64_t b,int64_t c,int64_t d,int64_t e,int64_t f,int64_t g,int64_t h,int64_t i,int64_t j) {
    return a+b+c+d+e+f+g+h+i+j;
}
int main(void) {
    for (int flag=0; flag<2; ++flag) for (int x=-3; x<=3; ++x) {
        int64_t want=(flag?0:x)+2*(17+31+255+65535)+(flag?0:x)+(flag?17:31)+x;
        if (remat_probe(flag,x)!=want) return 1;
    }
    __int128 x=((__int128)123<<64)+456;
    if (remat_shift(x)!=(x<<7)) return 2;
    return 0;
}
