/* Identical freestanding C input for every compiler/profile. All arithmetic
 * is unsigned; host lengths are bounded by the runner. No pointer call ABI. */
#include <stdint.h>
_Static_assert(sizeof(unsigned int)==4 && sizeof(void*)==4, "wasm32");
struct arena_type { unsigned char before[8]; unsigned char data[1024]; unsigned char after[8]; };
static struct arena_type arena;
unsigned int object_offset(void) { return (unsigned int)(uintptr_t)&arena; }
unsigned int buffer_offset(void) { return (unsigned int)(uintptr_t)&arena.data[0]; }
unsigned int scalar(unsigned int x, unsigned int n) {
    return ((x * 1664525u + 1013904223u) ^ (x >> 13)) + n;
}
unsigned long long wide(unsigned long long x, unsigned int n) {
    return ((x * 6364136223846793005ull + 1442695040888963407ull) ^ (x >> 33)) + n;
}
unsigned int integer_loop(unsigned int x, unsigned int n) {
    for (unsigned int i=0;i<n;i++) x=(x*1664525u+1013904223u)^(x>>13);
    return x;
}
unsigned int branch_switch(unsigned int x, unsigned int n) {
    for (unsigned int i=0;i<n;i++) {
        if (x&1u) x=x*3u+1u; else x=x>>1;
        switch(x&3u) { case 0: x+=17u; break; case 1: x^=0x9e3779b9u; break; case 2: x*=5u; break; default: x-=7u; }
    }
    return x;
}
unsigned int checksum(unsigned int x, unsigned int n) {
    for (unsigned int i=0;i<n;i++) x=(x*33u)^arena.data[i&1023u];
    return x;
}
unsigned int pointer_mutate(unsigned int x, unsigned int n) {
    /* Scalar address round trip is deliberate; every access stays in data. */
    unsigned char *p=(unsigned char *)(uintptr_t)buffer_offset();
    for (unsigned int i=0;i<n;i++) { unsigned int j=i&1023u; p[j]=(unsigned char)(p[j]+x+i); x=(x*33u)^p[j]; }
    return x;
}
unsigned int local_array(unsigned int x, unsigned int n) {
    unsigned int a[16];
    for(unsigned int i=0;i<16;i++) a[i]=x+i;
    for(unsigned int i=0;i<n;i++) { unsigned int j=i&15u; a[j]=(a[j]*33u)^x; x+=a[j]; }
    return x;
}
static unsigned int recursive_helper(unsigned int x, unsigned int depth) {
    volatile unsigned int a[2]; a[0]=x; a[1]=depth;
    if(depth==0) return a[0]^0x5a5a5a5au;
    return recursive_helper(x+depth,depth-1)+a[0];
}
unsigned int recursion(unsigned int x, unsigned int n) {
    for(unsigned int i=0;i<n;i++) x=recursive_helper(x,8);
    return x;
}
