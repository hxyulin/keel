extern "C" {
#include "clobber.h"
void *null_invoke(int);
int null_may_throw(int x) {
    clobber();
    if (x) throw x;
    return 0;
}
}
int main() {
    if (null_invoke(0)) return 1;
    if (null_invoke(7)) return 2;
    return 0;
}
