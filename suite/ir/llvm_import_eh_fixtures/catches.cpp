// Deliberately uses runtime int RTTI and external definitions: no ODR objects.
extern "C" int eh_throw(int);
extern "C" void eh_cleanup(int) noexcept;
struct Guard { int value; ~Guard() noexcept { eh_cleanup(value); } };
extern "C" int eh_catches(int x) {
  try { return eh_throw(x) + 3; }
  catch (int value) { return value + 101; }
  catch (...) { return -303; }
}
extern "C" int eh_typed(int x) {
  try { return eh_throw(x) + 5; }
  catch (int value) { return value + 201; }
}
extern "C" int eh_cleaned(int x) {
  Guard guard{x};
  return eh_throw(x) + 7;
}
extern "C" int eh_rethrow(int x) {
  try { return eh_throw(x) + 9; }
  catch (int) { eh_cleanup(x); throw; }
}
