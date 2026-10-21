using U = unsigned long long;
extern "C" U boundary(U);
extern "C" U cpp_case(U a, U b, volatile U *p) {
    U result = boundary(a) ^ boundary(b);
    *p = result;
    return result + a;
}
