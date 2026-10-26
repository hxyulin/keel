#include <stdint.h>
#include <stdio.h>
extern unsigned long datum, zeros[4];
extern const unsigned char asm_bytes[4];
extern unsigned long target(unsigned long), asm_leaf(unsigned long,unsigned long), call_assembly(unsigned long,unsigned long);
struct record { unsigned long (*function)(unsigned long); unsigned long *object; unsigned long (*kept)(void); };
extern const struct record records;
extern const int32_t relative_record;
int main(void) {
    for(unsigned i=0;i<4;i++) if(zeros[i]) { puts("FAIL zero storage"); return 1; }
    const unsigned char expected[]={3,5,7,11};
    for(unsigned i=0;i<4;i++) if(asm_bytes[i]!=expected[i]) { puts("FAIL assembly data"); return 1; }
    if(records.function!=target || records.object!=&datum || records.kept()!=41) { puts("FAIL relocations"); return 1; }
    if((uintptr_t)&relative_record + (intptr_t)relative_record != (uintptr_t)target) { puts("FAIL relative relocation"); return 1; }
    uint64_t state=0x123456789abcdef;
    for(unsigned i=0;i<512;i++) {
        state^=state<<13; state^=state>>7; state^=state<<17;
        unsigned long a=state,b=state*0xdeadbeefUL;
        datum=i;
        if(target(a)!=a+i+23 || asm_leaf(a,b)!=((a+b)^37UL)
           || call_assembly(a,b)!=((a+b)^37UL)+a+i+23) { puts("FAIL call result"); return 1; }
    }
    puts("PASS"); return 0;
}
