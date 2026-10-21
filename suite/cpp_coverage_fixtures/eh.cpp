using U = unsigned long long;
struct Error {};
extern "C" U cpp_case(U a, U b, volatile U *) {
    try { if (a & 1) throw Error{}; return b; }
    catch (const Error &) { return a; }
}
