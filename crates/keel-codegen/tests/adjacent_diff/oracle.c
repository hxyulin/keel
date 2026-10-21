// Correctness only: run under the caller's normal fast_opt_locked.py lock.
// Compiles unchanged on Linux x86_64 and native Apple ARM64.
#define _GNU_SOURCE
#include <assert.h>
#include <inttypes.h>
#include <limits.h>
#include <signal.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/wait.h>
#include <unistd.h>

typedef uint64_t (*kernel)(const void *,const void *,const void *,uint64_t);
extern uint64_t rk_adjacent(const void *,const void *,const void *,uint64_t);
extern uint64_t rk_adjacent_scalar(const void *,const void *,const void *,uint64_t);
static unsigned cases, faults, unroll_values, unroll_faults;
static uint64_t oracle(const unsigned char *a,uint64_t n) {
    uint64_t sum=0;
    for (uint64_t i=1;i<n;i++) {
        int32_t x,y;
        memcpy(&x,a+4*i,4); memcpy(&y,a+4*(i-1),4);
        int64_t d=(int64_t)x-(int64_t)y;
        sum+=(uint64_t)(d<0?-d:d);
    }
    return sum;
}
static void check(const unsigned char *a,uint64_t n) {
    uint64_t want=oracle(a,n), got=rk_adjacent(a,a,a,n);
    if (got!=want) { fprintf(stderr,"n=%"PRIu64" expected=%"PRIu64" got=%"PRIu64"\n",n,want,got); abort(); }
    assert(rk_adjacent_scalar(a,a,a,n)==want);
    ++cases;
}
static int output_fd;
static void fault(int sig,siginfo_t *info,void *unused) {
    (void)sig;
    (void)unused;
    uintptr_t addr=(uintptr_t)info->si_addr;
    (void)!write(output_fd,&addr,sizeof(addr));
    _exit(0);
}
static uintptr_t first_fault(kernel f,const void *a,uint64_t n) {
    int p[2]; assert(pipe(p)==0);
    pid_t child=fork(); assert(child>=0);
    if (!child) {
        close(p[0]); output_fd=p[1];
        struct sigaction action={0}; action.sa_sigaction=fault;
        action.sa_flags=SA_SIGINFO; sigemptyset(&action.sa_mask);
        assert(sigaction(SIGSEGV,&action,0)==0); assert(sigaction(SIGBUS,&action,0)==0);
        volatile uint64_t result=f(a,a,a,n); (void)result;
        _exit(9);
    }
    close(p[1]); uintptr_t addr=0;
    assert(read(p[0],&addr,sizeof(addr))==sizeof(addr)); close(p[0]);
    int status; assert(waitpid(child,&status,0)==child);
    assert(WIFEXITED(status)&&WEXITSTATUS(status)==0);
    return addr;
}
static void compare_fault(const void *a,uint64_t n) {
    uintptr_t want=first_fault(rk_adjacent_scalar,a,n);
    uintptr_t got=first_fault(rk_adjacent,a,n);
    if (got!=want) { fprintf(stderr,"fault n=%"PRIu64" expected=%"PRIxPTR" got=%"PRIxPTR"\n",n,want,got); abort(); }
    ++faults;
}
int main(void) {
    size_t page=(size_t)sysconf(_SC_PAGESIZE);
    unsigned char *region=mmap(0,3*page,PROT_NONE,MAP_PRIVATE|MAP_ANON,-1,0);
    assert(region!=MAP_FAILED); unsigned char *mem=region+page;
    assert(mprotect(mem,page,PROT_READ|PROT_WRITE)==0);
    for (unsigned offset=0;offset<16;offset++) {
        unsigned char *a=mem+offset;
        for (unsigned pattern=0;pattern<5;pattern++) {
            uint32_t state=0xa5b35791u;
            for (unsigned i=0;i<300;i++) {
                state=state*1664525u+1013904223u;
                int32_t edge[]={INT32_MIN,INT32_MAX,-1,0,1};
                int32_t v=pattern==0?edge[i%5]:pattern==1?(i%2?INT32_MIN:INT32_MAX):
                    pattern==2?(int32_t)state:pattern==3?0:(int32_t)i;
                memcpy(a+4*i,&v,4);
            }
            for (uint64_t n=0;n<=257;n++) check(a,n);
        }
    }
    // Null and inaccessible inputs are valid for the exact n=0/1 no-read guard.
    check(0,0); check(0,1); check(region,0); check(region,1);
    for (uint64_t n=2;n<=257;n++) {
        unsigned char *a=mem+page-4*n;
        check(a,n); // last byte immediately precedes the guard page
    }
    // Explicit zero/one/two/three-main-trip boundaries and every residue.
    // Distinct extrema per lane expose lost accumulators and wrong history
    // at main-to-main, main-to-pair and pair-to-tail transitions.
    for (unsigned offset=0;offset<16;offset++) {
        unsigned char *a=mem+offset;
        for (unsigned i=0;i<64;i++) {
            int32_t values[]={INT32_MIN,7,INT32_MAX,-31,0,INT32_MIN,73,INT32_MAX};
            int32_t v=values[i%8]; memcpy(a+4*i,&v,4);
        }
        const uint64_t boundaries[]={8,9,10,11,16,17,18,19};
        for (unsigned i=0;i<sizeof(boundaries)/sizeof(*boundaries);i++) {
            check(a,boundaries[i]); ++unroll_values;
        }
        for (uint64_t groups=0;groups<=3;groups++)
            for (uint64_t residue=0;residue<8;residue++) {
                uint64_t n=2+8*groups+residue;
                check(a,n); ++unroll_values;
                check(mem+page-4*n,n); ++unroll_values;
            }
    }
    // Aligned cases reach each main-loop load in the first, second and third groups.
    // Misaligned cases also compare straddling faults; when lane=0, a preceding
    // sample may fault first (including the prefix in the first group).
    // n includes the entire group, preventing pair cleanup in the main cases.
    // Equal fault addresses check agreement, not independent proof of load order.
    for (uint64_t group=0;group<3;group++)
        for (uint64_t load=0;load<4;load++)
            for (uint64_t lane=0;lane<2;lane++)
                for (unsigned misalign=0;misalign<4;misalign++) {
                    uint64_t valid=2+8*group+2*load+lane;
                    compare_fault(mem+page-4*valid+misalign,2+8*(group+1));
                    ++unroll_faults;
                }
    // Pair cleanup and scalar tail after a completed unrolled group, all
    // residues; the extra sample must fault exactly as the scalar does.
    for (uint64_t residue=0;residue<8;residue++) {
        uint64_t valid=26+residue;
        compare_fault(mem+page-4*valid,valid+1); ++unroll_faults;
    }
    // The very first scalar read faults before a[0]; also test a[0] faulting
    // after a valid a[1], then vector chunks and scalar tails reaching a guard.
    compare_fault(mem+page-4,2);
    compare_fault(mem-4,2);
    for (uint64_t valid=2;valid<=17;valid++) {
        compare_fault(mem+page-4*valid,valid+1);
        compare_fault(mem+page-4*valid,valid+2);
        for (unsigned misalign=1;misalign<4;misalign++)
            compare_fault(mem+page-4*valid+misalign,valid+1);
    }
    // Guards must retain scalar faults for index-shift and pointer-end wrap.
    compare_fault(mem+page-4,UINT64_MAX);
    compare_fault((void *)(UINTPTR_MAX-3),2);
    // Both guard boundaries must keep the original scalar first read.
    compare_fault(mem+page-4,UINT64_MAX/4);
    compare_fault(mem+page-4,UINT64_MAX/4+1);
    compare_fault((void *)(UINTPTR_MAX-31),10);
    compare_fault((void *)(UINTPTR_MAX-7),3);
    assert(munmap(region,3*page)==0);
    printf("{\"status\":\"pass\",\"value_cases\":%u,\"first_fault_cases\":%u,\"unroll_value_cases\":%u,\"unroll_first_fault_cases\":%u,\"performance_measured\":false}\n",cases,faults,unroll_values,unroll_faults);
    return 0;
}
