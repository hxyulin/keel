using U = unsigned long long;
U mix(U a, U b) { return a ^ b; }
unsigned mix(unsigned a, unsigned b) { return a + b; }
extern "C" U cpp_case(U a, U b, volatile U *) {
    return mix(a, b) + mix(static_cast<unsigned>(a), static_cast<unsigned>(b));
}
