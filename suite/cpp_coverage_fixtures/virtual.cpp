using U = unsigned long long;
struct Base { virtual U apply(U) = 0; };
struct Derived : Base { U apply(U a) override { return a ^ 19; } };
__attribute__((noinline)) U dispatch(Base *p, U a) { return p->apply(a); }
extern "C" U cpp_case(U a, U, volatile U *) { Derived d; return dispatch(&d, a); }
