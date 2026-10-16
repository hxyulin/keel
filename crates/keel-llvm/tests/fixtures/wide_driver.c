#include <stdint.h>
#include <stdio.h>
extern uint64_t overflow_umul_128(uint64_t,uint64_t),overflow_smul_128(uint64_t,uint64_t);
int main(void) {
    uint64_t v[]={0,1,127,128,UINT64_MAX,UINT64_C(0x8000000000000000),UINT64_C(0x7fffffffffffffff)};
    for(unsigned i=0;i<sizeof(v)/sizeof(v[0]);i++) for(unsigned j=0;j<sizeof(v)/sizeof(v[0]);j++) {
        uint64_t a=v[i],b=v[j];
        unsigned __int128 u;
        __int128 s;
        unsigned uf=__builtin_mul_overflow((unsigned __int128)(__int128)(int64_t)a,(unsigned __int128)(__int128)(int64_t)b,&u);
        unsigned sf=__builtin_mul_overflow((__int128)(int64_t)a,(__int128)(int64_t)b,&s);
        uint64_t actual_u=overflow_umul_128(a,b),actual_s=overflow_smul_128(a,b);
        if(actual_u!=((uint64_t)u^uf) || actual_s!=((uint64_t)s^sf)) return 1;
        printf("%016llx %016llx\n",(unsigned long long)actual_u,(unsigned long long)actual_s);
    }
    return 0;
}
