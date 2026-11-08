#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>
#include <stdio.h>
extern size_t probe_prefix(const uint8_t*,const uint8_t*,size_t);
extern void hash_word(uint64_t*,uint64_t),hash_bytes(uint64_t*,const uint8_t*,size_t);
extern size_t hand_prefix(const uint8_t*,const uint8_t*,size_t);
extern void hand_word(uint64_t*,uint64_t),hand_bytes(uint64_t*,const uint8_t*,size_t);
static uint64_t byte(uint64_t h,uint8_t b) { return(h^b)*UINT64_C(1099511628211); }
static void check(int ok) { if(!ok)abort(); }
static size_t prefix(const uint8_t*a,const uint8_t*b,size_t n) {
    const volatile uint8_t *x=a,*y=b;size_t i=0;
    while(i<n&&x[i]==y[i])i++;
    return i;
}
int main(void) {
    uint8_t a[256],b[256];
    check(probe_prefix(0,0,0)==0);check(hand_prefix(0,0,0)==0);
    hash_bytes(0,0,0);hand_bytes(0,0,0);
    const size_t lengths[]={0,1,2,3,4,5,6,7,8,9,15,16,17,31,32,33,63,64,65,127,128,129};
    const unsigned aligns[]={0,1,7};
    for(unsigned ni=0;ni<sizeof(lengths)/sizeof(lengths[0]);ni++) {
      size_t n=lengths[ni];
      const size_t positions[]={0,1,2,3,4,5,6,7,8,9,15,16,17,n/2,n?n-1:0,n};
      for(unsigned pi=0;pi<sizeof(positions)/sizeof(positions[0]);pi++)for(unsigned ai=0;ai<sizeof(aligns)/sizeof(aligns[0]);ai++) {
        size_t pos=positions[pi];unsigned align=aligns[ai];if(pos>n)continue;
        for(unsigned i=0;i<256;i++)a[i]=b[i]=(uint8_t)(i*137+17);
        if(pos<n)b[align+pos]^=1;
        size_t want=prefix(a+align,b+align,n);
        check(probe_prefix(a+align,b+align,n)==want);
        check(hand_prefix(a+align,b+align,n)==want);
    }
    }
    size_t page=(size_t)sysconf(_SC_PAGESIZE);
    uint8_t *x=mmap(0,page*2,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANON,-1,0);
    uint8_t *y=mmap(0,page*2,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANON,-1,0);
    check(x!=MAP_FAILED&&y!=MAP_FAILED);
    check(!mprotect(x+page,page,PROT_NONE)&&!mprotect(y+page,page,PROT_NONE));
    for(unsigned ni=0;ni<sizeof(lengths)/sizeof(lengths[0]);ni++) {
        size_t n=lengths[ni];
        memset(x+page-n,77,n);memset(y+page-n,77,n);
        check(probe_prefix(x+page-n,y+page-n,n)==n);
        check(hand_prefix(x+page-n,y+page-n,n)==n);
    }
    // A mismatch in the first wide read must prevent another wide read even
    // when the supplied bound is longer than the accessible prefix.
    memset(x+page-8,77,8);memset(y+page-8,77,8);y[page-1]^=1;
    check(probe_prefix(x+page-8,y+page-8,4096)==7);
    check(hand_prefix(x+page-8,y+page-8,4096)==7);
    munmap(x,page*2);munmap(y,page*2);
    const unsigned seeds[]={0,1,17,127,128,255};
    const unsigned hash_lengths[]={0,1,7,8,9,16,64};
    const unsigned offsets[]={0,1,7,8};
    for(unsigned si=0;si<sizeof(seeds)/sizeof(seeds[0]);si++) {
        unsigned seed=seeds[si];
        uint64_t v=(UINT64_C(0x9e3779b97f4a7c15)*seed),h=~v,want=h,c=h;
        for(unsigned i=0;i<8;i++)want=byte(want,(uint8_t)(v>>(i*8)));
        hash_word(&h,v);hand_word(&c,v);check(h==want&&c==want);
        for(unsigned ni=0;ni<sizeof(hash_lengths)/sizeof(hash_lengths[0]);ni++)for(unsigned oi=0;oi<sizeof(offsets)/sizeof(offsets[0]);oi++) {
            unsigned n=hash_lengths[ni],offset=offsets[oi];
            // The byte array aliases a real uint64_t object's representation.
            // Volatile accesses keep the independent oracle's stores ordered.
            union { uint64_t h;uint8_t b[80]; } got,hand,oracle;
            for(unsigned i=0;i<80;i++)got.b[i]=hand.b[i]=oracle.b[i]=(uint8_t)(seed+i*137);
            volatile uint64_t *vh=&oracle.h;
            volatile uint8_t *vb=oracle.b;
            for(unsigned i=0;i<n;i++)*vh=byte(*vh,vb[offset+i]);
            hash_bytes(&got.h,got.b+offset,n);hand_bytes(&hand.h,hand.b+offset,n);
            check(!memcmp(got.b,oracle.b,80)&&!memcmp(hand.b,oracle.b,80));
        }
    }
    puts("prefix, hash-word and aliasing byte-hash contracts passed");
}
