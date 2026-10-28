using U = unsigned long long;
extern "C" U cpp_case(U a, U, volatile U *) {
    // Bounded inputs avoid signed overflow but Clang still emits nsw.
    long long v = static_cast<long long>(a & 0xffff);
    return static_cast<U>((v + 1) * (v + 3));
}
