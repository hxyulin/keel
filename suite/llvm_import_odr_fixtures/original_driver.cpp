#include <cassert>
#include <cstdio>
using U = unsigned long long;
extern "C" U cpp_case(U, U, volatile U *);
int main() {
  U xs[] = {0, 1, 17, 31, 1024, ~0ULL};
  for (U x : xs) {
#if CASE == 0
    U expected = (x * 32) ^ 13;
#elif CASE == 1
    U expected = x ^ 19;
#else
    U expected = (x & 1) ? x : 13;
#endif
    assert(cpp_case(x, 13, nullptr) == expected);
  }
  std::puts("original: 6 independently asserted inputs");
}
