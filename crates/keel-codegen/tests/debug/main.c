#include <stdint.h>
int64_t calc(int64_t x);
int64_t nocol(int64_t x);
int main(void) {
    return calc(5) + nocol(1) != 18;
}
