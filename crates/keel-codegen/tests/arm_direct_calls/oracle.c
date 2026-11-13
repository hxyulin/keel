#define _GNU_SOURCE
#include <stdint.h>
#include <stdlib.h>
#include <unwind.h>
#include <dlfcn.h>
extern uint64_t direct_bridge(uint64_t);
extern uint64_t fallback_bridge(uint64_t);
extern uint64_t recursive(uint64_t);
extern uint64_t reverse(uint64_t,uint64_t,uint64_t,uint64_t,uint64_t,uint64_t,uint64_t,uint64_t);
extern int64_t narrow_calls(uint64_t);
extern uint64_t indirect(void*,uint64_t,uint64_t);
extern uint32_t indirect_void(void*,void*,uint32_t);
static _Unwind_Reason_Code visit(struct _Unwind_Context *context, void *data) {
    Dl_info info;
    uintptr_t ip = (uintptr_t)_Unwind_GetIP(context);
    if (ip && dladdr((void*)(ip - 1), &info)) {
        if (info.dli_saddr == (void*)direct_bridge) *(unsigned*)data |= 1;
        if (info.dli_saddr == (void*)fallback_bridge) *(unsigned*)data |= 2;
    }
    return _URC_NO_REASON;
}
__attribute__((noinline)) uint64_t capture(uint64_t value) {
    unsigned seen = 0;
    _Unwind_Backtrace(visit, &seen);
    if (seen != 3) abort();
    return value + 1;
}
uint64_t external_eight(uint64_t a,uint64_t b,uint64_t c,uint64_t d,uint64_t e,uint64_t f,uint64_t g,uint64_t h) {
    return a + 3*b + 5*c + 7*d + 11*e + 13*f + 17*g + 19*h;
}
int8_t narrow_probe(int8_t a,uint16_t b,int32_t c) {
    return (uint64_t)(int64_t)a + b + (uint64_t)(int64_t)c;
}
static uint64_t reference_recursive(uint64_t n) {
    return n < 2 ? n : reference_recursive(n - 1) + reference_recursive(n - 2) + n;
}
static uint64_t combine(uint64_t a,uint64_t b) { return a*1009 + b; }
static void put(void *p,uint32_t x) { *(uint32_t*)p = x ^ UINT32_C(0xfedcba98); }
int main(void) {
    if (direct_bridge(0) != 0) return 1;
    for (uint64_t x=1;x<100;x++) if (direct_bridge(x) != x + (x/17)*2 + 1) return 2;
    for (uint64_t n=0;n<14;n++) if (recursive(n) != reference_recursive(n)) return 3;
    if (reverse(1,2,3,4,5,6,7,8) != external_eight(8,7,6,5,4,3,2,1)+1) return 4;
    uint64_t values[]={0,1,127,128,255,256,32767,32768,65535,UINT64_C(0x80000000),UINT64_MAX};
    for (unsigned i=0;i<sizeof values/sizeof values[0];i++) {
        uint64_t x=values[i];
        int8_t first=narrow_probe(x,x,x),second=narrow_probe(first,x,x);
        if (narrow_calls(x) != (int64_t)first + second) return 5;
        if (indirect((void*)combine,x,17) != combine(17,x)/17 + x) return 6;
        uint32_t output=0;
        if (indirect_void((void*)put,&output,x) != ((uint32_t)x ^ UINT32_C(0xfedcba98))) return 7;
        if (output != ((uint32_t)x ^ UINT32_C(0xfedcba98))) return 8;
    }
    return 0;
}
