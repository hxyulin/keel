using U = unsigned long long;
extern "C" U cpp_case(U a, U b, volatile U *p) {
    U old = *p;
    *p = a ^ b;
    return ((a + b) * 7) ^ (a >> 3) ^ old;
}
