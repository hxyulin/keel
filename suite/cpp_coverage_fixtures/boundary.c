typedef unsigned long long U;
/* Separate C object calls back into a C-ABI C++ export (no LTO). */
extern U cpp_case(U, U, volatile U *);
U boundary(U a) { return (a ^ 0x123456789abcdef0ULL) + 17; }
U through_c(U a, U b, volatile U *p) { return cpp_case(a, b, p); }
