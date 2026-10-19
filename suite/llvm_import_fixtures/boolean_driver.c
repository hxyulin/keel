#include <stdint.h>
extern int64_t bool_sext(uint64_t);
extern uint64_t pointer_bit(void *);
extern unsigned char narrow_return(uint64_t);
int main(void) {
    for (uint64_t i = 0; i < 65536; ++i) {
        if (bool_sext(i) != (i & 1 ? -1 : 0)) return 1;
        if (pointer_bit((void *)(uintptr_t)i) != (i & 1)) return 2;
        if (narrow_return(i) != (unsigned char)i) return 3;
    }
    return 0;
}
