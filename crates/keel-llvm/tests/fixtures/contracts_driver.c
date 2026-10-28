#include <stdint.h>
#include <stdio.h>
extern uint64_t read_only(const uint64_t *, const uint64_t *);
extern uint64_t write_distinct(const uint64_t *, uint64_t *);
extern uint64_t pointer_equality(const void *, const void *);
extern uint64_t masked(const void *, uint64_t, uint64_t, uint64_t);
int main(void) {
    uint64_t a=17,b=4;
    if (read_only(&a,&a)!=34) return 1;
    if (write_distinct(&a,&b)!=34 || a!=17 || b!=9) return 2;
    if (pointer_equality(&a,&a)!=1 || pointer_equality(0,0)!=1) return 3;
    if (masked(0,0,INT64_MAX,1)!=7) return 4;
    if (masked(&a,1,2,3)!=5) return 5;
    puts("contracts ok");
    return 0;
}
