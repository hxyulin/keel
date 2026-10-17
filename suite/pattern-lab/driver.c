#define _POSIX_C_SOURCE 200809L
#include <errno.h>
#include <inttypes.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#if defined(__x86_64__)
#include <cpuid.h>
#endif
extern uint64_t pattern_kernel(uint64_t, uint64_t, const uint64_t *, uint64_t);
extern uint64_t pattern_oracle(uint64_t, uint64_t, const uint64_t *, uint64_t);
static volatile uint64_t consumed;
static int number(const char *s, uint64_t *v) {
    if (!*s) return 0;
    for (const char *p=s; *p; ++p) if (*p<'0'||*p>'9') return 0;
    errno=0; char *end; unsigned long long n=strtoull(s,&end,10);
    if (errno || *end) return 0;
    *v=(uint64_t)n; return 1;
}
static int ns(uint64_t *v) {
    struct timespec t;
    if (clock_gettime(CLOCK_MONOTONIC,&t) || t.tv_sec<0 || t.tv_nsec<0 || t.tv_nsec>=1000000000L || (uint64_t)t.tv_sec>(UINT64_MAX-999999999)/1000000000) return 0;
    *v=(uint64_t)t.tv_sec*1000000000+(uint64_t)t.tv_nsec; return 1;
}
static int tsc_supported(void) {
#if defined(__x86_64__)
    unsigned a,b,c,d;
    if (__get_cpuid_max(0x80000000,0)<0x80000007) return 0;
    __cpuid(0x80000001,a,b,c,d); if (!(d&(1u<<27))) return 0;
    __cpuid(0x80000007,a,b,c,d); return !!(d&(1u<<8));
#else
    return 0;
#endif
}
static uint64_t stamp(unsigned *aux) {
#if defined(__x86_64__)
    unsigned lo,hi,cpu;
    __asm__ volatile("lfence\n\trdtscp\n\tlfence" : "=a"(lo),"=d"(hi),"=c"(cpu) :: "memory");
    *aux=cpu; return ((uint64_t)hi<<32)|lo;
#else
    *aux=0; return 0;
#endif
}
int main(int argc, char **argv) {
    uint64_t it,seed,words,mask;
    if (argc!=6 || !number(argv[1],&it)||!number(argv[2],&seed)||!number(argv[3],&words)||!number(argv[4],&mask)||!words||words>1048576||mask>=words) {
        fputs("invalid arguments\n",stderr); return 2;
    }
    int ticks=!strcmp(argv[5],"tsc");
    if (!ticks && strcmp(argv[5],"monotonic")) return 2;
    if (ticks && !tsc_supported()) { puts("{\"status\":\"unsupported\",\"reason\":\"RDTSCP/invariant TSC unavailable\"}"); return 3; }
    uint64_t *data=malloc((size_t)words*sizeof(*data)); if (!data) return 2;
    uint64_t x=UINT64_C(0x243f6a8885a308d3);
    for (uint64_t i=0;i<words;i++) { x=x*UINT64_C(6364136223846793005)+UINT64_C(1442695040888963407); data[i]=x; }
    uint64_t expected=pattern_oracle(it,seed,data,mask);
    uint64_t wall0,wall1,t0=0,t1=0; unsigned cpu0=0,cpu1=0;
    if (!ns(&wall0)) { free(data); return 2; }
    if (ticks) t0=stamp(&cpu0);
    __asm__ volatile("" ::: "memory");
    uint64_t result=pattern_kernel(it,seed,data,mask);
    __asm__ volatile("" ::: "memory");
    if (ticks) t1=stamp(&cpu1);
    if (!ns(&wall1) || wall1<wall0 || (ticks && t1<t0)) { free(data); return 2; }
    consumed=result; free(data);
    if (result!=expected || (!it && result!=seed)) { fprintf(stderr,"checksum mismatch: got=%"PRIu64" expected=%"PRIu64"\n",result,expected); return 4; }
    if (ticks && cpu0!=cpu1) { fputs("logical CPU/TSC_AUX changed\n",stderr); return 5; }
    printf("{\"status\":\"ok\",\"iterations\":%"PRIu64",\"seed\":%"PRIu64",\"checksum\":%"PRIu64",\"oracle\":%"PRIu64",\"wall_ns\":%"PRIu64",\"elapsed\":%"PRIu64",\"unit\":\"%s\",\"tsc_aux\":%u}\n",it,seed,result,expected,wall1-wall0,ticks?t1-t0:wall1-wall0,ticks?"ticks":"ns",cpu0);
    return 0;
}
