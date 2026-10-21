#include <assert.h>
unsigned long long use_state(unsigned long long), use_state_right(unsigned long long);
int main(void) {
    assert(use_state(3) == 3);
    assert(use_state_right(7) == 10);
    assert(use_state(0) == 10);
}
