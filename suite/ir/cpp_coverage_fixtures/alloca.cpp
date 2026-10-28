using U = unsigned long long;
extern "C" U cpp_case(U a, U b, volatile U *p) { U v = a + b; *p = v; return v; }
