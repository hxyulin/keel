#pragma once
#include <typeinfo>
#define NOINLINE __attribute__((noinline))
inline NOINLINE int inline_helper(int x) { return x * 3 + 7; }
template<class T> NOINLINE T template_helper(T x) { return x * 5 + 11; }
extern template int template_helper<int>(int);
inline int shared_value = 17;
inline NOINLINE int *static_storage() { static int value = 23; return &value; }
using Fn = int (*)(int);
inline Fn function_slot = &inline_helper;
struct Base { virtual int apply(int) = 0; };
struct Derived : Base { NOINLINE int apply(int x) override { return x ^ 19; } };
struct Error {};
extern "C" void cleanup_tick() noexcept;
struct Guard { NOINLINE ~Guard() noexcept { cleanup_tick(); } };
using Dispatch = int (*)(Base *, int);
using Thrower = int (*)(int);
extern "C" int native_dispatch(Base *, int);
extern "C" int native_throw(int);
#define API(S) \
extern "C" Fn S##_inline(); \
extern "C" Fn S##_template(); \
extern "C" int *S##_value(); \
extern "C" int *S##_static(); \
extern "C" Fn *S##_slot(); \
extern "C" const void *S##_vtable(); \
extern "C" const std::type_info *S##_derived_type(); \
extern "C" const std::type_info *S##_error_type(); \
extern "C" int S##_dispatch(Base *, int); \
extern "C" int S##_provide(Dispatch, int); \
extern "C" int S##_cross(int); \
extern "C" int S##_throw(int); \
extern "C" int S##_catch(int, int);
API(left)
API(right)
