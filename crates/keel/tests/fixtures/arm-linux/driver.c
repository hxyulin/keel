#include "abi.h"
#include <string.h>
#include <stdio.h>
_Static_assert(sizeof(va_list)==32,"Linux ARM va_list");
_Static_assert(sizeof(long double)==16,"binary128 storage");
_Static_assert((char)-1 > 0,"unsigned plain char");
extern int tls_thread_check(void);
int main(void) {
    if(stack_mix(1,2,3,4,5,6,7,8,250,-123,1024,10000)!=11187) return 1;
    __int128 wide=((__int128)1<<90)+23;
    if(wide_add(3,wide,7)!=wide+10) return 2;
    struct pair p={11,22}; p=pair_swap(p); if(p.a!=22||p.b!=11) return 3;
    struct big b={10,20,30,40}; b=big_add(b); if(b.a!=11||b.b!=22||b.c!=33||b.d!=44) return 4;
    struct hfa h={1.5,2.5}; h=hfa_swap(h); if(h.a!=2.5||h.b!=1.5) return 5;
    if(hfa_stack(1,2,3,4,5,6,7,8,h,9)!=49) return 6;
    if(sum_ints(10,1L,2L,3L,4L,5L,6L,7L,8L,9L,10L)!=55) return 7;
    if(sum_doubles(10,0.5,1.,2.,3.,4.,5.,6.,7.,8.,9.,10.)!=55.5) return 8;
    if(sum_wide(5,wide,wide,wide,wide,wide)!=wide*5) return 9;
    if(copy_args(2,3L,4L)!=3334) return 10;
    char out[128]; if(format_args(out,"%ld/%.1f/%s",123L,2.5,"arm")!=11 || strcmp(out,"123/2.5/arm")) return 11;
    if(char_is_unsigned((char)255)!=255) return 12;
    if(tls_next()!=7 || tls_next()!=8) return 13;
    if(tls_thread_check()) return 14;
    if(tls_next()!=9) return 15;
    struct aligned_pair aligned={11,22}; if(aligned_sum(3,aligned,4)!=40) return 16;
    if(hfa_boundary(1,2,3,4,5,6,7,h,8)!=40) return 17;
    puts("ARM Linux ABI ok"); return 0;
}
