#include "shared.hpp"
// Explicit instantiations produce real weak_odr definitions; inline functions
// and inline data produce linkonce_odr on the supported Clang target.
template int template_helper<int>(int);
// Retain an unreferenced definition: whole-module import must not prune it.
template<class T> NOINLINE T unused_helper(T x) { return x + 37; }
template int unused_helper<int>(int);
#define JOIN_(a,b) a##b
#define JOIN(a,b) JOIN_(a,b)
#define SELF(n) JOIN(SIDE,n)
#define PEER(n) JOIN(OTHER,n)
extern "C" Fn SELF(_inline)() { return &inline_helper; }
extern "C" Fn SELF(_template)() { return &template_helper<int>; }
extern "C" int *SELF(_value)() { return &shared_value; }
extern "C" int *SELF(_static)() { return static_storage(); }
extern "C" Fn *SELF(_slot)() { return &function_slot; }
extern "C" const void *SELF(_vtable)() { Derived d; return *reinterpret_cast<void **>(&d); }
extern "C" const std::type_info *SELF(_derived_type)() { return &typeid(Derived); }
extern "C" const std::type_info *SELF(_error_type)() { return &typeid(Error); }
extern "C" NOINLINE int SELF(_dispatch)(Base *p, int x) { return p->apply(x); }
extern "C" int SELF(_provide)(Dispatch call, int x) { Derived d; return call(&d, x); }
extern "C" int SELF(_cross)(int x) { Derived d; return PEER(_dispatch)(&d, x); }
extern "C" int SELF(_throw)(int mode) {
  Guard guard;
  if (mode == 1) throw Error{};
  if (mode == 2) throw 41;
  return 29;
}
extern "C" int SELF(_catch)(int source, int mode) {
  Guard guard;
  try {
    if (source == 0) return left_throw(mode) + 3;
    if (source == 1) return right_throw(mode) + 3;
    return native_throw(mode) + 3;
  }
  catch (const Error &) { return 101; }
  catch (...) { return 203; }
}
