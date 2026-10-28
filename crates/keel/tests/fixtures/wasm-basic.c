#include <stdint.h>
#include <stddef.h>
_Static_assert(sizeof(long)==4 && sizeof(void*)==4 && sizeof(size_t)==4, "ILP32");
#if defined(__APPLE__) || defined(__linux__) || defined(__x86_64__) || defined(__aarch64__) || __STDC_HOSTED__
#error host contamination
#endif
static int numbers[3] = {11,22,33};
static int *link = numbers+1;
static unsigned long helper(unsigned long x) { return x*3+1; }
unsigned long checksum(unsigned int address, int length) {
    unsigned char *input = (unsigned char *)(uintptr_t)address;
    unsigned char local[16];
    unsigned long sum=7;
    for(int i=0;i<16;i++) local[i]=(unsigned char)(i^0x5a);
    for(int i=0;i<length;i++) sum=(sum*33)^input[i];
    for(int i=0;i<16;i++) sum^=(unsigned long)local[i]<<(i&7);
    return sum+helper(length)+*link;
}
int local_array(void) { int a[4]={7,0,3,4}; return a[0]+a[1]+a[2]+a[3]; }
int recurse(int n) { int a[2]={n,3}; if(n<=0) return a[1]; return a[0]+recurse(n-1); }
