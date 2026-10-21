using U = unsigned long long;
extern "C" U boundary(U);
struct Guard {
    volatile U *p;
    U value;
    ~Guard() { *p = boundary(value); }
};
extern "C" U cpp_case(U a, U b, volatile U *p) {
    Guard guard{p, a};
    return a ^ b;
}
