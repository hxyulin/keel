using U = unsigned long long;
extern "C" U cpp_case(U a, U b, volatile U *p) { p[1] = a; return p[b & 1]; }
