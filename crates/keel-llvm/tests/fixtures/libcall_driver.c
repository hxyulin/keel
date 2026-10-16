#include <stdio.h>
extern volatile unsigned hits;
extern unsigned long probe(const char *),probe_constant(void);
int main(void) {
    unsigned before=hits;
    unsigned long unused_result=probe("x");
    if(hits!=before+1 || unused_result!=0) return 1;
    unsigned long constant_result=probe_constant();
    if(hits!=before+2 || constant_result!=7) return 2;
    printf("hits=%u unused=%lu constant=%lu\n",hits-before,unused_result,constant_result);
    return 0;
}
