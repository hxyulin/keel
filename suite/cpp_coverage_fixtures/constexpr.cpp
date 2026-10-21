using U = unsigned long long;
constexpr U factor(unsigned n) { return n ? 2 * factor(n - 1) : 1; }
static_assert(factor(5) == 32, "constant evaluation");
extern "C" U cpp_case(U a, U b, volatile U *) { constexpr U scale = factor(5); return a * scale ^ b; }
