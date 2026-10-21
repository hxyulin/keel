#include "shared.hpp"
#include <cassert>
#include <cstdio>
static int cleanups;
extern "C" void cleanup_tick() noexcept { ++cleanups; }
extern "C" NOINLINE int native_dispatch(Base *p, int x) { return p->apply(x); }
extern "C" int native_throw(int mode) {
  Guard guard;
  if (mode == 1) throw Error{};
  if (mode == 2) throw 41;
  return 29;
}
static int native_catch(int source, int mode) {
  Guard guard;
  try {
    if (source == 0) return left_throw(mode) + 3;
    if (source == 1) return right_throw(mode) + 3;
    return native_throw(mode) + 3;
  }
  catch (const Error &) { return 101; }
  catch (...) { return 203; }
}
int main() {
  assert(left_inline() == right_inline() && left_inline() == &inline_helper);
  assert(left_template() == right_template());
  assert(left_value() == right_value() && left_value() == &shared_value);
  assert(left_static() == right_static() && left_static() == static_storage());
  assert(left_slot() == right_slot() && *left_slot() == left_inline());
  assert(*left_value() == 17 && *left_static() == 23);
  *left_value() = 47; *right_static() = 53;
  assert(*right_value() == 47 && *left_static() == 53);
  Derived native;
  assert(left_vtable() == right_vtable());
  assert(left_vtable() == *reinterpret_cast<void **>(&native));
  assert(left_derived_type() == right_derived_type());
  assert(left_derived_type() == &typeid(native));
  assert(left_error_type() == right_error_type() && left_error_type() == &typeid(Error));
  // Itanium address point: the RTTI relocation is immediately before methods.
  assert(reinterpret_cast<const void *const *>(left_vtable())[-1] == left_derived_type());
  int inputs[] = {0, 1, 19, 31, 127, 1024};
  for (int x : inputs) {
    assert(left_inline()(x) == x * 3 + 7);
    assert(right_inline()(x) == x * 3 + 7);
    assert(left_template()(x) == x * 5 + 11);
    assert(right_template()(x) == x * 5 + 11);
    assert((*left_slot())(x) == x * 3 + 7);
    assert(left_dispatch(&native, x) == (x ^ 19));
    assert(right_dispatch(&native, x) == (x ^ 19));
    assert(left_provide(native_dispatch, x) == (x ^ 19));
    assert(right_provide(native_dispatch, x) == (x ^ 19));
    assert(left_provide(right_dispatch, x) == (x ^ 19));
    assert(right_provide(left_dispatch, x) == (x ^ 19));
    assert(left_cross(x) == (x ^ 19) && right_cross(x) == (x ^ 19));
  }
  using Catcher = int (*)(int, int);
  Catcher catchers[] = {left_catch, right_catch, native_catch};
  for (Catcher catcher : catchers) for (int source = 0; source != 3; ++source)
    for (int mode = 0; mode != 3; ++mode) {
      cleanups = 0;
      int result = catcher(source, mode);
      assert(result == (mode == 0 ? 32 : mode == 1 ? 101 : 203));
      assert(cleanups == 2);
    }
  std::puts("ODR: identity, storage, dispatch; 27 EH paths independently asserted");
}
