using U = unsigned long long;
extern "C" U boundary(U);
extern "C" U cpp_case(U a, U b, volatile U *) {
    if (a & 1) return boundary(a) + b;
    return boundary(b) ^ a;
}
