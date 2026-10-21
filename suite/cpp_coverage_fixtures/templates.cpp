using U = unsigned long long;
template<class T, T Factor> T mix(T a, T b) { return (a + b) * Factor; }
extern "C" U cpp_case(U a, U b, volatile U *) { return mix<U, 7>(a, b); }
