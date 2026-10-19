using U = unsigned long long;
using S = long long;
extern "C" U mix(U a, U b) { return (a + b) * 7 ^ (a >> 3); }
extern "C" U choose(U a, U b) { return a < b ? a ^ b : a + b; }
extern "C" S signed_div(S a) { return a / 3; }
extern "C" S signed_rem(S a) { return a % -5; }
extern "C" S narrow_sign(unsigned char a) { return (signed char)a; }
extern "C" U memory(volatile U *p, U a) {
    U old = *p;
    *p = a;
    return old ^ a;
}
extern "C" __attribute__((noinline)) U helper(U a, U b) { return (a ^ b) + 19; }
extern "C" U callhelper(U a, U b) { return helper(a, b) ^ a; }
