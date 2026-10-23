// Freestanding input: no C++ library, exceptions, TLS, or unsupported relocs.
#ifndef UNIT
#define UNIT a
#endif
#define CAT_(a,b) a##b
#define CAT(a,b) CAT_(a,b)
template<int N> __attribute__((noinline)) int answer() { return N; }
inline int shared = 19;
inline __attribute__((noinline)) int read_shared() { return shared; }
extern "C" int CAT(invoke_,UNIT)() { return answer<23>() + read_shared(); }
