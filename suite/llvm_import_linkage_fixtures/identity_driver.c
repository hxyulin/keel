#include <assert.h>
void *identity_left(void), *identity_right(void);
int main(void) { assert(identity_left() == identity_right()); }
