#include <stdint.h>
#include <setjmp.h>
#include "clobber.h"
int64_t null_probe(int64_t, int64_t, void *);
void *null_return(void);
void *null_setjmp(void *);
void *null_stackrestore(int64_t);
int64_t null_branch(void *);
int64_t null_cycle(int64_t, int64_t, int64_t);
int64_t null_observe(void *a,void *b,void *c,void *d,void *e,void *f,void *g,void *h,void *i,void *j) {
    if (a || c || d || e || f || g || h || i || j) return -100000;
    return (int64_t)(uintptr_t)b;
}
int main(void) {
    for (int a=-2; a<=2; ++a) for (int b=-2; b<=2; ++b) {
        int64_t want = a<b ? 0 : 2468;
        if (null_probe(a,b,(void *)(uintptr_t)1234) != want) return 1;
    }
    if (null_return()) return 2;
    if (null_branch(0)!=37 || null_branch((void *)(uintptr_t)1234)!=91) return 4;
    jmp_buf env;
    if (null_setjmp(env)) return 5;
    if (null_stackrestore(128)) return 6;
    for (int n=1; n<=6; ++n)
        if (null_cycle(17,9,n) != (n%2 ? 8 : -8)) return 3;
    return 0;
}
