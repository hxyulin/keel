using U = unsigned long long;
extern "C" U cpp_case(U a, U b, volatile U *memory) {
    try {
        if (a & 1) throw static_cast<int>(a & 255);
        return b;
    } catch (int value) {
        memory[0] = static_cast<U>(value);
        return static_cast<U>(value) ^ b;
    }
}
