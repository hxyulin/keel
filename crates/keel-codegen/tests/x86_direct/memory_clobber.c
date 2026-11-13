#include <stddef.h>
#if defined(__clang__)
#define RAW __attribute__((optnone,noinline))
#elif defined(__GNUC__)
#define RAW __attribute__((optimize("no-tree-loop-distribute-patterns"),noinline))
#else
#define RAW
#endif
#define CLOBBER() __asm__ volatile("xorq %%rax,%%rax\n xorq %%rcx,%%rcx\n xorq %%rdx,%%rdx\n xorq %%rdi,%%rdi\n xorq %%rsi,%%rsi\n xorq %%r8,%%r8\n xorq %%r9,%%r9\n xorq %%r10,%%r10\n xorq %%r11,%%r11":::"rax","rcx","rdx","rdi","rsi","r8","r9","r10","r11","memory","cc")
RAW void *memcpy(void *dst,const void *src,size_t n) {
    volatile unsigned char *d=dst;const volatile unsigned char *s=src;
    for(size_t i=0;i<n;i++)d[i]=s[i];CLOBBER();return dst;
}
RAW void *memmove(void *dst,const void *src,size_t n) {
    volatile unsigned char *d=dst;const volatile unsigned char *s=src;
    if((size_t)d<(size_t)s){for(size_t i=0;i<n;i++)d[i]=s[i];}else{for(size_t i=n;i;i--)d[i-1]=s[i-1];}
    CLOBBER();return dst;
}
RAW void *memset(void *dst,int byte,size_t n) {
    volatile unsigned char *d=dst;for(size_t i=0;i<n;i++)d[i]=(unsigned char)byte;
    CLOBBER();return dst;
}
