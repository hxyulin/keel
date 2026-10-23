#include <stdint.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>
#include <stdio.h>
static unsigned csr(void) { unsigned x; __asm__ volatile("stmxcsr %0":"=m"(x)); return x; }
__attribute__((noinline)) void clobber(void) {
 __asm__ volatile("pxor %%xmm0,%%xmm0\npxor %%xmm1,%%xmm1\npxor %%xmm2,%%xmm2\npxor %%xmm3,%%xmm3\npxor %%xmm4,%%xmm4\npxor %%xmm5,%%xmm5\npxor %%xmm6,%%xmm6\npxor %%xmm7,%%xmm7\npxor %%xmm8,%%xmm8\npxor %%xmm9,%%xmm9\npxor %%xmm10,%%xmm10\npxor %%xmm11,%%xmm11\npxor %%xmm12,%%xmm12\npxor %%xmm13,%%xmm13\npxor %%xmm14,%%xmm14\npxor %%xmm15,%%xmm15"
 ::: "xmm0","xmm1","xmm2","xmm3","xmm4","xmm5","xmm6","xmm7","xmm8","xmm9","xmm10","xmm11","xmm12","xmm13","xmm14","xmm15");
}
struct Case { void (*run)(const void*,void*); uint64_t offset; unsigned wide,mode; };
