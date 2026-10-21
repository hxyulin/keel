#include <assert.h>
unsigned long long replaceable(unsigned long long x) { return x ^ 91; }
unsigned long long interposition_entry(unsigned long long);
int main(void) { assert(interposition_entry(3) == (3 ^ 91)); }
