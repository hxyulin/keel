extern "C" int catch_compact();
extern "C" void thrower() { throw 42; }
int main() { return catch_compact() != 47; }
