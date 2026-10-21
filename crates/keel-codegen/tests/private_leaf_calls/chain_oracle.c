#include <stdint.h>
#include <stdio.h>
extern uint64_t caller(const uint64_t *);
extern uint64_t chain_caller(const uint64_t *);
extern uint64_t unknown_caller(const uint64_t *);
extern uint64_t recursive_caller(const uint64_t *);
extern uint64_t shuffle_caller(const uint64_t *);
__attribute__((noinline)) uint64_t external_noise(uint64_t x) {
    __asm__ volatile("mov x9, #91\n mov x10, #92\n mov x11, #93\n mov x12, #94\n mov x13, #95\n mov x14, #96\n mov x15, #97" ::: "x9","x10","x11","x12","x13","x14","x15");
    return x ^ UINT64_C(0xfedcba9876543210);
}
int main(void) {
    uint64_t state=1,a[12];
    for (unsigned n=0;n<10000;n++) {
        uint64_t sum=0;
        for(unsigned i=0;i<12;i++) { state=state*UINT64_C(6364136223846793005)+1; a[i]=n<4?(n==0?0:n==1?UINT64_MAX:n==2?UINT64_C(0x8000000000000000):i):state;sum+=a[i]; }
        uint64_t first=a[0]+1;
        if(caller(a)!=sum+first || chain_caller(a)!=sum+first || unknown_caller(a)!=sum+(a[0]^UINT64_C(0xfedcba9876543210)) || recursive_caller(a)!=sum || shuffle_caller(a)!=sum+8*a[0]) { fprintf(stderr,"mismatch at %u\n",n); return 1; }
    }
    return 0;
}
