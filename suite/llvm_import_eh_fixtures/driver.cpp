#include <cassert>
#include <cstdio>
extern "C" int eh_catches(int);
extern "C" int eh_typed(int);
extern "C" int eh_cleaned(int);
extern "C" int eh_rethrow(int);
extern "C" int eh_edges(int, int);
static int cleaned, last;
extern "C" void eh_cleanup(int x) noexcept { ++cleaned; last = x; }
extern "C" int eh_throw(int x) {
  if (x < 0) throw -x;
  if (x == 1000) throw 42.5;
  return x * 2;
}
static int observed(int (*f)(int), int x) {
  try { return f(x); }
  catch (int n) { assert(n == -x && x < 0); return -701; }
  catch (double n) { assert(n == 42.5 && x == 1000); return -702; }
}
int main() {
  int cases = 0;
  for (int x = -128; x <= 1001; ++x) {
    assert(eh_catches(x) == (x < 0 ? -x + 101 : x == 1000 ? -303 : 2*x + 3));
    assert(observed(eh_typed, x) == (x < 0 ? -x + 201 : x == 1000 ? -702 : 2*x + 5));
    cleaned = 0; last = 0;
    assert(observed(eh_cleaned, x) == (x < 0 ? -701 : x == 1000 ? -702 : 2*x + 7));
    assert(cleaned == 1 && last == x);
    cleaned = 0; last = 0;
    assert(observed(eh_rethrow, x) == (x < 0 ? -701 : x == 1000 ? -702 : 2*x + 9));
    assert(cleaned == (x < 0 ? 1 : 0));
    if (x < 0) assert(last == x);
    for (int side = 0; side <= 1; ++side) {
      // Distinct PHIs on both invoke edges; invoke result is a normal PHI input.
      int base = side ? 23 : 11;
      int expected = x < 0 || x == 1000 ? (x ^ base) + 100 : 2*x + base;
      int got = eh_edges(x, side);
      if (got != expected) std::fprintf(stderr, "edges x=%d side=%d got=%d expected=%d\n", x, side, got, expected);
      assert(got == expected);
    }
    ++cases;
  }
  std::printf("EH: %d independently asserted inputs\n", cases);
}
