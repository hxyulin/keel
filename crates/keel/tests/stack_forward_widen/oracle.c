#include <stdint.h>
#include <stddef.h>
#include <string.h>
#include <stdlib.h>
#include <signal.h>
#include <sys/mman.h>
#include <sys/wait.h>
#include <sys/resource.h>
#include <unistd.h>
#ifdef __linux__
#include <sys/prctl.h>
#endif
extern void before_plain(void *,void *), after_plain(void *,void *);
extern void before_snapshot(void *,void *), after_snapshot(void *,void *);
extern void footprint(void *,void *);
static unsigned evaluations, mutations;
static volatile unsigned *published;
void *once(void *p) { evaluations++; return p; }
void clobber(void *p) {
    mutations++; memset(p,0x42,16);
#if defined(__x86_64__)
    __asm__ volatile(
        "pxor %%xmm0, %%xmm0\n"
        "pxor %%xmm1, %%xmm1\n"
        "pxor %%xmm2, %%xmm2\n"
        "pxor %%xmm3, %%xmm3\n"
        "pxor %%xmm4, %%xmm4\n"
        "pxor %%xmm5, %%xmm5\n"
        "pxor %%xmm6, %%xmm6\n"
        "pxor %%xmm7, %%xmm7\n"
        "pxor %%xmm8, %%xmm8\n"
        "pxor %%xmm9, %%xmm9\n"
        "pxor %%xmm10, %%xmm10\n"
        "pxor %%xmm11, %%xmm11\n"
        "pxor %%xmm12, %%xmm12\n"
        "pxor %%xmm13, %%xmm13\n"
        "pxor %%xmm14, %%xmm14\n"
        "pxor %%xmm15, %%xmm15\n"
        ::: "xmm0", "xmm1", "xmm2", "xmm3", "xmm4", "xmm5", "xmm6", "xmm7", "xmm8", "xmm9", "xmm10", "xmm11", "xmm12", "xmm13", "xmm14", "xmm15");
#elif defined(__aarch64__)
    __asm__ volatile(
        "movi v0.16b, #0\n"
        "movi v1.16b, #0\n"
        "movi v2.16b, #0\n"
        "movi v3.16b, #0\n"
        "movi v4.16b, #0\n"
        "movi v5.16b, #0\n"
        "movi v6.16b, #0\n"
        "movi v7.16b, #0\n"
        // AAPCS64 preserves only the low 64 bits of v8-v15. Exercise their
        // volatile upper halves too, without violating the callback ABI.
        "mov v8.d[1], xzr\n"
        "mov v9.d[1], xzr\n"
        "mov v10.d[1], xzr\n"
        "mov v11.d[1], xzr\n"
        "mov v12.d[1], xzr\n"
        "mov v13.d[1], xzr\n"
        "mov v14.d[1], xzr\n"
        "mov v15.d[1], xzr\n"
        "movi v16.16b, #0\n"
        "movi v17.16b, #0\n"
        "movi v18.16b, #0\n"
        "movi v19.16b, #0\n"
        "movi v20.16b, #0\n"
        "movi v21.16b, #0\n"
        "movi v22.16b, #0\n"
        "movi v23.16b, #0\n"
        "movi v24.16b, #0\n"
        "movi v25.16b, #0\n"
        "movi v26.16b, #0\n"
        "movi v27.16b, #0\n"
        "movi v28.16b, #0\n"
        "movi v29.16b, #0\n"
        "movi v30.16b, #0\n"
        "movi v31.16b, #0\n"
        ::: "v0", "v1", "v2", "v3", "v4", "v5", "v6", "v7", "v8", "v9", "v10", "v11", "v12", "v13", "v14", "v15", "v16", "v17", "v18", "v19", "v20", "v21", "v22", "v23", "v24", "v25", "v26", "v27", "v28", "v29", "v30", "v31");
#endif
}
void later(void) { *published=1; }
static unsigned char *guard(long page) {
    unsigned char *p=mmap(0,3*page,PROT_NONE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);
    if(p==MAP_FAILED || mprotect(p+page,page,PROT_READ|PROT_WRITE)) exit(90);
    return p;
}
static int fault(void *out, void *input, unsigned char *shared, long page) {
    memset(shared,0xa5,16); *published=0;
    pid_t pid=fork(); if(pid<0)return 91;
    if(!pid) { signal(SIGSEGV,SIG_DFL); signal(SIGBUS,SIG_DFL); footprint(out,input); _exit(99); }
    int status; if(waitpid(pid,&status,0)!=pid)return 92;
    if(!WIFSIGNALED(status)||(WTERMSIG(status)!=SIGSEGV&&WTERMSIG(status)!=SIGBUS))return 93;
    if(*published)return 94;
    (void)page; return 0;
}
int main(void) {
    struct rlimit core={0,0}; if(setrlimit(RLIMIT_CORE,&core))return 89;
#ifdef __linux__
    // Ubuntu's apport ignores RLIMIT_CORE; non-dumpable processes skip it.
    if(prctl(PR_SET_DUMPABLE,0))return 89;
#endif
    long page=sysconf(_SC_PAGESIZE);
    unsigned char *a=guard(page), *d=guard(page);
    unsigned char *shared=mmap(0,page,PROT_READ|PROT_WRITE,MAP_SHARED|MAP_ANONYMOUS,-1,0);
    if(shared==MAP_FAILED)return 90;
    published=(volatile unsigned *)(shared+32);
    uint32_t seed=17;
    void (*fn[])(void*,void*)={before_plain,after_plain,before_snapshot,after_snapshot};
    for(unsigned t=0;t<2048;t++) {
        _Alignas(16) int32_t input[4];
        for(unsigned lane=0;lane<4;lane++) {
            seed=seed*1664525u+1013904223u;
            uint32_t edge[]={0,1,0xffffffffu,0x80000000u,0x7fffffffu};
            uint32_t bits=t<625?edge[(t/(lane==0?1:lane==1?5:lane==2?25:125))%5]:seed;
            memcpy(&input[lane],&bits,4);
        }
        int64_t expected[2]={input[0],input[1]};
        for(unsigned which=0;which<4;which++)for(unsigned pos=0;pos<2;pos++)for(unsigned alias=0;alias<2;alias++) {
            unsigned char *p=a+page+(pos?page-16:0);
            unsigned char *out=alias?p:d+page+(pos?0:page-16);
            memset(a+page,0xa5,page); memset(d+page,0xa5,page); memcpy(p,input,16);
            unsigned e=evaluations, m=mutations;
            fn[which](out,p);
            if(evaluations!=e+(which>=2)||mutations!=m+(which>=2))return 1;
            if(memcmp(out,expected,16))return 2;
            // Every byte outside the exact objects remains a canary. Protected
            // pages on either side catch overreads and overwrites at both edges.
            // The footprint does not depend on lane values, so scan the pages
            // only for the 625 edge inputs.
            for(long n=0;t<625&&n<page;n++) {
                unsigned char *q=a+page+n;
                if((q<p||q>=p+16)&&*q!=0xa5)return 3;
                q=d+page+n;
                if((alias||q<out||q>=out+16)&&*q!=0xa5)return 4;
            }
            if(!alias) {
                unsigned char changed[16]; memset(changed,0x42,16);
                if(memcmp(p,which>=2?(void*)changed:(void*)input,16))return 5;
            }
        }
    }
    // This is an explicitly align-1 IR witness, not undefined misaligned C.
    // Eight readable input bytes must still fault on the original 16-byte load,
    // before publication or the later external call becomes observable.
    int r=fault(shared,a+2*page-8,shared,page); if(r)return r;
    for(unsigned i=0;i<16;i++)if(shared[i]!=0xa5)return 95;
    // A full input at the page edge succeeds and writes exactly sixteen bytes.
    _Alignas(16) int32_t v[4]={INT32_MIN,INT32_MAX,-1,17}; memcpy(a+2*page-16,v,16);
    footprint(d+2*page-16,a+2*page-16);
    int64_t expected[2]={INT32_MIN,INT32_MAX};
    if(memcmp(d+2*page-16,expected,16)||*published!=1)return 96;
    // Only eight writable output bytes must fault before the later call.
    r=fault(d+2*page-8,a+2*page-16,shared,page); if(r)return r;
    return 0;
}
