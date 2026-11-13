#include <stdint.h>
#include <unwind.h>
#define TEN uint64_t,uint64_t,uint64_t,uint64_t,uint64_t,uint64_t,uint64_t,uint64_t,uint64_t,uint64_t
extern uint64_t local10(TEN),large(TEN),trace_frame(TEN),large_homes(uint64_t);
extern uint64_t indirect10(uint64_t (*)(TEN),TEN);
extern uint64_t mixed10(uint64_t,uint64_t,uint64_t,uint64_t,uint64_t,uint64_t,int8_t,uint16_t,uint32_t,uint64_t *);
extern uint64_t call_void(uint64_t *,uint64_t);
extern uint64_t narrow_stack(uint64_t *);
static unsigned seen;
static _Unwind_Reason_Code visit(struct _Unwind_Context *context,void *argument) {
    (void)argument;uintptr_t region=(uintptr_t)_Unwind_GetRegionStart(context);
    if(region==(uintptr_t)large)seen|=1;
    if(region==(uintptr_t)trace_frame)seen|=2;
    return _URC_NO_REASON;
}
uint64_t trace_large(void) {seen=0;_Unwind_Backtrace(visit,0);return seen;}
uint64_t foreign10(uint64_t a,uint64_t b,uint64_t c,uint64_t d,uint64_t e,uint64_t f,uint64_t g,uint64_t h,uint64_t i,uint64_t j) {
    uint64_t result=a+2*b+3*c+4*d+5*e+6*f+7*g+8*h+9*i+10*j;
    __asm__ volatile("xorq %%rax,%%rax\n xorq %%rcx,%%rcx\n xorq %%rdx,%%rdx\n xorq %%rdi,%%rdi\n xorq %%rsi,%%rsi\n xorq %%r8,%%r8\n xorq %%r9,%%r9\n xorq %%r10,%%r10\n xorq %%r11,%%r11":::"rax","rcx","rdx","rdi","rsi","r8","r9","r10","r11","memory","cc");
    return result;
}
__attribute__((naked)) uint64_t observer10(uint64_t a,uint64_t b,uint64_t c,uint64_t d,uint64_t e,uint64_t f,int8_t g,uint16_t h,uint32_t i,uint64_t *j) {
    __asm__("cmpl $-1,8(%rsp)\n jne 1f\n cmpl $65535,16(%rsp)\n jne 1f\n cmpl $0x80000000,24(%rsp)\n jne 1f\n movq 32(%rsp),%rax\n movq (%rax),%rax\n ret\n1: xorq %rax,%rax\n ret");
}
void write8(uint64_t a,uint64_t b,uint64_t c,uint64_t d,uint64_t e,uint64_t f,uint64_t g,uint64_t *p) {*p=a+b+c+d+e+f+g;}
int main(void) {
    uint64_t state=UINT64_C(0x9e3779b97f4a7c15);
    for(unsigned run=0;run<32;run++) {
        uint64_t a[10],forward=0,reverse=0;
        for(unsigned i=0;i<10;i++){state=state*UINT64_C(6364136223846793005)+1;a[i]=state;}
        for(unsigned i=0;i<10;i++){forward+=a[i]*(i+1);reverse+=a[9-i]*(i+1);}
#define VALUES a[0],a[1],a[2],a[3],a[4],a[5],a[6],a[7],a[8],a[9]
        if(local10(VALUES)!=forward)return 1;
        if(large(VALUES)!=2*reverse+forward+3+2*a[9])return 2;
        if(indirect10(foreign10,VALUES)!=reverse)return 3;
        if(mixed10(1,2,3,4,5,6,(int8_t)state,(uint16_t)state,(uint32_t)state,&state)!=21+(uint64_t)(int64_t)(int8_t)state+(uint16_t)state+(uint32_t)state+state)return 4;
        if(narrow_stack(&state)!=20+UINT64_C(65535)+UINT64_C(2147483648)+2*state)return 5;
        if(large_homes(state)!=state*601)return 6;
        uint64_t memory=state;if(call_void(&memory,state)!=state+21||memory!=state+21)return 7;
#undef VALUES
    }
    return 0;
}
