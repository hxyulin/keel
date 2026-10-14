#include <stdint.h>
extern uint64_t checked_fold(void);
int main(void) {
    return checked_fold() != (UINT64_C(1) << 63 | UINT64_C(1));
}
