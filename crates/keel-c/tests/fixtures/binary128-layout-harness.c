/* Native GCC/Clang own the objects; Keel only computes pointers/layout. */
#include <string.h>
#include <stdint.h>
typedef __float128 Q;
typedef _Complex float C __attribute__((__mode__(__TC__)));
struct Wrapper { char before; C value; char after; };
union Payload { C values[2]; Q real; };
struct Nested { char before; union Payload payload; char after; };
extern C *tc_element(C*,int), *tc_field(struct Wrapper*), *tc_nested(struct Nested*,int);
extern Q *q_element(Q*,int);
extern char *tc_after(struct Wrapper*);
extern void tc_store(struct Wrapper*,char);
extern int tc_layout(void);
int main(void) {
    struct Wrapper w[3]; struct Nested n;
    unsigned char expected[sizeof(w)]; C c[3]; Q q[3];
    memset(w,0xa5,sizeof(w)); memcpy(expected,w,sizeof(w));
    for (int i=0;i<3;i++) {
        if (tc_element(c,i)!=&c[i] || q_element(q,i)!=&q[i]) return 1;
        if (tc_field(&w[i])!=&w[i].value || tc_after(&w[i])!=&w[i].after) return 2;
        if ((uintptr_t)tc_field(&w[i])%16) return 3;
        tc_store(&w[i],0x37);
        expected[i*sizeof(struct Wrapper)+48]=0x37;
    }
    if (memcmp(expected,w,sizeof(w))) return 4;
    for (int i=0;i<2;i++) if (tc_nested(&n,i)!=&n.payload.values[i]) return 5;
    return tc_layout()!=112;
}
