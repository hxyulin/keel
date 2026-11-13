#include <stdint.h>
extern uint64_t chain(uint64_t,uint64_t),recursive(uint64_t),narrow_args(void),void_call(uint32_t);
extern uint64_t indirect(int8_t (*)(int8_t,uint16_t,uint32_t),int8_t,uint16_t,uint32_t);
uint64_t foreign6(uint64_t a,uint64_t b,uint64_t c,uint64_t d,uint64_t e,uint64_t f) {
    uint64_t result=a+3*b+5*c+7*d+11*e+13*f;
    __asm__ volatile("xorq %%rax,%%rax\n xorq %%rcx,%%rcx\n xorq %%rdx,%%rdx\n xorq %%rdi,%%rdi\n xorq %%rsi,%%rsi\n xorq %%r8,%%r8\n xorq %%r9,%%r9\n xorq %%r10,%%r10\n xorq %%r11,%%r11":::"rax","rcx","rdx","rdi","rsi","r8","r9","r10","r11","memory","cc");
    return result;
}
__attribute__((naked)) uint64_t narrow_observer(int8_t a,uint16_t b) {
    __asm__("cmpl $-1,%edi\n jne 1f\n cmpl $65535,%esi\n jne 1f\n movl $1,%eax\n ret\n1: xorl %eax,%eax\n ret");
}
__attribute__((naked)) uint64_t alignment(void) {
    __asm__("movq %rsp,%rax\n andl $15,%eax\n cmpl $8,%eax\n sete %al\n movzbl %al,%eax\n ret");
}
void poke(uint32_t *p,uint32_t x) {*p=x+1;}
static int8_t target(int8_t a,uint16_t b,uint32_t c) {return (int8_t)((int32_t)a+(uint32_t)b+c);}
int main(void) {
    uint64_t state=UINT64_C(0x9e3779b97f4a7c15);
    for(unsigned i=0;i<64;i++) {
        state=state*UINT64_C(6364136223846793005)+1;
        uint64_t a=state,b=~state;
        uint64_t external=a+3*b+5*a+7*b+11*a+13*b;
        uint64_t local=b+a+(b^a)-b*a;
        if(chain(a,b)!=a*b+local+external/3) return 1;
        if(void_call((uint32_t)state)!=(uint64_t)((uint32_t)state+1)+1) return 2;
        uint8_t expected=(uint8_t)target((int8_t)state,(uint16_t)state,(uint32_t)state);
        if(indirect(target,(int8_t)state,(uint16_t)state,(uint32_t)state)!=expected) return 3;
    }
    uint64_t factorial=1;
    for(unsigned n=0;n<=12;n++) {if(n)factorial*=n;if(recursive(n)!=factorial)return 4;}
    if(narrow_args()!=1) return 5;
    return 0;
}
