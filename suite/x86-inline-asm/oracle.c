#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#define U unsigned long
#define BINARY(n) extern U n(U,U), ref_##n(U,U);
BINARY(asm_q_views)
BINARY(asm_q_rex)
BINARY(asm_flags)
BINARY(asm_scratch)
BINARY(asm_add)
BINARY(asm_fixed)
BINARY(asm_mul)
BINARY(asm_early)
#define UNARY(n) extern U n(U), ref_##n(U);
unsigned char asm_deferred_anchor[64];
UNARY(asm_deferred_number)
UNARY(asm_deferred_address)
UNARY(asm_memory_aligned)
extern U asm_memory_alias_inputs(U*,U*), ref_asm_memory_alias_inputs(U*,U*);
UNARY(asm_memory_local)
UNARY(asm_memory_write)
extern U asm_memory_read(U*), ref_asm_memory_read(U*);
extern U asm_memory_mixed(U*,U,U), ref_asm_memory_mixed(U*,U,U);
extern U asm_memory_once(U*), ref_asm_memory_once(U*);
extern U asm_memory_alias(U*,U*,U), ref_asm_memory_alias(U*,U*,U);
extern void asm_memory_unused(U*), ref_asm_memory_unused(U*);
extern void asm_memory_widths(unsigned char*), ref_asm_memory_widths(unsigned char*);
extern void asm_memory_array(U*), ref_asm_memory_array(U*);
UNARY(asm_q_output)
UNARY(asm_address_width)
UNARY(asm_address_symbol)
UNARY(asm_address_constant)
UNARY(asm_negated)
UNARY(asm_negated_min)
extern U asm_address_read(U*), ref_asm_address_read(U*);
extern U *asm_q_pointer(U*), *ref_asm_q_pointer(U*);
UNARY(asm_macro)
UNARY(asm_aligned)
UNARY(asm_views)
UNARY(asm_constants)
UNARY(asm_unique)
UNARY(asm_loop)
UNARY(asm_unused)
UNARY(asm_remat)
extern U asm_six(U,U,U,U,U,U), ref_asm_six(U,U,U,U,U,U);
extern U asm_clobber(U,U,U,U,U,U,U,U), ref_asm_clobber(U,U,U,U,U,U,U,U);
extern U asm_memory_barrier(U*),ref_asm_memory_barrier(U*);
extern U asm_once(U*,U*), ref_asm_once(U*,U*);
extern U *asm_pointer(U*), *ref_asm_pointer(U*);
extern unsigned char asm_byte(unsigned char,unsigned char),ref_asm_byte(unsigned char,unsigned char);
extern unsigned short asm_word(unsigned short,unsigned short),ref_asm_word(unsigned short,unsigned short);
extern unsigned int asm_dword(unsigned int,unsigned int),ref_asm_dword(unsigned int,unsigned int);
#define CHECK(n, ...) do { U got=n(__VA_ARGS__), want=ref_##n(__VA_ARGS__); if(got!=want){printf("FAIL %s %lu: %lx != %lx\n",#n,i,got,want);return 1;} } while(0)
static uint64_t state=0xabcde123456789ULL;
static U next(void){state^=state<<13;state^=state>>7;state^=state<<17;return state;}
int main(void) {
    _Static_assert(sizeof(U)==8,"LP64 required");
    for(U i=0;i<512;i++) {
        U a=next(),b=next(),c=next(),d=next(),e=next(),f=next(),g=next(),h=next();
        if(i<5){static const U edge[]={0,1,~0UL,0x8000000000000000UL,0x7fffffffffffffffUL};a=edge[i];b=edge[4-i];}
        CHECK(asm_q_output,a);CHECK(asm_q_views,a,b);CHECK(asm_q_rex,a,b);
        CHECK(asm_address_width,a);CHECK(asm_address_symbol,a);CHECK(asm_address_constant,a);
        CHECK(asm_negated,a);CHECK(asm_negated_min,a);
        CHECK(asm_memory_aligned,a);CHECK(asm_memory_local,a);CHECK(asm_memory_write,a);
        CHECK(asm_deferred_number,a);CHECK(asm_deferred_address,a);
        CHECK(asm_flags,a,b);CHECK(asm_scratch,a,b);CHECK(asm_macro,a);CHECK(asm_add,a,b);CHECK(asm_fixed,a,b);CHECK(asm_six,a,b,c,d,e,f);
        CHECK(asm_mul,a,b);CHECK(asm_early,a,b);CHECK(asm_views,a);
        CHECK(asm_aligned,a);CHECK(asm_constants,a);CHECK(asm_clobber,a,b,c,d,e,f,g,h);
        CHECK(asm_unique,a);CHECK(asm_loop,a);CHECK(asm_unused,a);CHECK(asm_remat,a);
        CHECK(asm_byte,a,b);CHECK(asm_word,a,b);CHECK(asm_dword,a,b);
        U x[]={c,a,b,d}, y[]={c,a,b,d};
        CHECK(asm_address_read,x+1);
        U got=asm_once(x+1,x+2),want=ref_asm_once(y+1,y+2);
        if(got!=want || memcmp(x,y,sizeof x)){puts("FAIL asm_once");return 1;}
        got=asm_memory_barrier(x+1);want=ref_asm_memory_barrier(y+1);
        if(got!=want || memcmp(x,y,sizeof x)){puts("FAIL asm_memory_barrier");return 1;}
        if(asm_q_pointer(x+1)!=ref_asm_q_pointer(x+1)){puts("FAIL asm_q_pointer");return 1;}
        if(asm_pointer(x+1)!=ref_asm_pointer(x+1)){puts("FAIL asm_pointer");return 1;}
        CHECK(asm_memory_read,x+1);
        got=asm_memory_mixed(x+1,a,b);want=ref_asm_memory_mixed(y+1,a,b);
        if(got!=want || memcmp(x,y,sizeof x)){puts("FAIL asm_memory_mixed");return 1;}
        got=asm_memory_once(x+1);want=ref_asm_memory_once(y+1);
        if(got!=want || memcmp(x,y,sizeof x)){puts("FAIL asm_memory_once");return 1;}
        got=asm_memory_alias(x+1,x+1,a);want=ref_asm_memory_alias(y+1,y+1,a);
        if(got!=want || memcmp(x,y,sizeof x)){puts("FAIL asm_memory_alias_same");return 1;}
        got=asm_memory_alias(x+1,x+2,b);want=ref_asm_memory_alias(y+1,y+2,b);
        if(got!=want || memcmp(x,y,sizeof x)){puts("FAIL asm_memory_alias_separate");return 1;}
        got=asm_memory_alias_inputs(x+1,x+1);want=ref_asm_memory_alias_inputs(y+1,y+1);
        if(got!=want || memcmp(x,y,sizeof x)){puts("FAIL asm_memory_alias_inputs_same");return 1;}
        got=asm_memory_alias_inputs(x+1,x+2);want=ref_asm_memory_alias_inputs(y+1,y+2);
        if(got!=want || memcmp(x,y,sizeof x)){puts("FAIL asm_memory_alias_inputs_separate");return 1;}
        asm_memory_unused(x+1);ref_asm_memory_unused(y+1);
        if(memcmp(x,y,sizeof x)){puts("FAIL asm_memory_unused");return 1;}
        asm_memory_widths((unsigned char *)(x+1));ref_asm_memory_widths((unsigned char *)(y+1));
        if(memcmp(x,y,sizeof x)){puts("FAIL asm_memory_widths");return 1;}
        asm_memory_array(x+1);ref_asm_memory_array(y+1);
        if(memcmp(x,y,sizeof x)){puts("FAIL asm_memory_array");return 1;}
    }
    puts("PASS");
    return 0;
}
