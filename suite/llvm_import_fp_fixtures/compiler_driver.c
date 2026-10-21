#include <assert.h>
#include <math.h>
extern double compiled(double, float), compiled_from(unsigned long long);
extern float compiled_narrow(double);
extern int compiled_to(double), compiled_cmp(double, double);
int main(void) {
 assert(compiled(3.0, 1.0f) == 2.0);
 assert(signbit(compiled(-0.0, -0.0f)));
 assert(compiled_narrow(16777217.0) == 16777216.0f);
 assert(compiled_from(9007199254740993ULL) == 9007199254740992.0);
 assert(compiled_to(-2.75) == -2);
 assert(compiled_cmp(NAN, 1.0) == 1);
 return 0;
}
