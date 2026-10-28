/* Imported exactly as emitted by Clang: FP signatures, calls, stack layout,
   arithmetic, comparisons and conversions without integer arithmetic promises. */
__attribute__((noinline)) double helper(double x, float y) { return x + (double)y; }
double compiled(double x, float y) { return helper(x, y) / 2.0; }
float compiled_narrow(double x) { return (float)x; }
double compiled_from(unsigned long long x) { return (double)x; }
int compiled_to(double x) { return (int)x; }
int compiled_cmp(double x, double y) { return x != y; }
