#include <stdint.h>
#include <string.h>
#include <assert.h>
#include <sys/mman.h>
#include <unistd.h>
extern uint32_t conditional32(const void*,uint64_t,int32_t,uint32_t);
extern uint64_t conditional64(const void*,uint64_t,int64_t,uint64_t);
int main(void) {
    size_t page=(size_t)sysconf(_SC_PAGESIZE);
    unsigned char *memory=mmap(0,page*2,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANON,-1,0);
    assert(memory!=MAP_FAILED && mprotect(memory+page,page,PROT_NONE)==0);
    uint64_t state=1;
    uint64_t patterns[]={0,1,UINT64_MAX,0x8000000000000000ULL,0x7fffffffffffffffULL,0x80000000,0x7fffffff,0xffffffff};
    for (unsigned bits=32;bits<=64;bits+=32) for (unsigned n=0;n<=257;n++) {
        unsigned char *p=memory+page-(size_t)n*(bits/8);
        for (unsigned j=0;j<n;j++) { state=state*6364136223846793005ULL+1; uint64_t v=j%3?state:patterns[j%8];memcpy(p+j*(bits/8),&v,bits/8); }
        for (unsigned k=0;k<8;k++) {
            uint64_t seed=patterns[k],cut=patterns[(k+3)%8];
            if(bits==32){uint32_t sum=(uint32_t)seed;for(unsigned j=0;j<n;j++){int32_t x;memcpy(&x,p+j*4,4);if(x<(int32_t)cut)sum+=(uint32_t)x;}assert(conditional32(p,n,(int32_t)cut,(uint32_t)seed)==sum);}
            else {uint64_t sum=seed;for(unsigned j=0;j<n;j++){int64_t x;memcpy(&x,p+j*8,8);if(x<(int64_t)cut)sum+=(uint64_t)x;}assert(conditional64(p,n,(int64_t)cut,seed)==sum);}
        }
    }
    assert(munmap(memory,page*2)==0);
    return 0;
}
