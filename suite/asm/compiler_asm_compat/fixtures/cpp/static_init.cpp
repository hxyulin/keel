// Dynamic global initialization order, function-local statics (guards), atexit destructors.
extern "C" int printf(const char *, ...);
extern "C" int atoi(const char *);

static int order[8];
static int order_n = 0;

struct Registered {
    int id, value;
    Registered(int id, int value) : id(id), value(value) { order[order_n++] = id; }
    ~Registered() { printf("dtor %d\n", id); }
};

static int compute(int x) { return x * x + order_n; }
Registered first(1, compute(3));
Registered second(2, first.value + 1);
static int derived_global = second.value * 10 + atoi("5");

__attribute__((noinline)) Registered &lazy(int seed) {
    static Registered local(9, seed * 2);
    return local;
}

struct Table { int v[16]; constexpr Table() : v() { for (int i = 0; i < 16; i++) v[i] = i * i; } };
constexpr Table table;

int main(int argc, char **) {
    int a = lazy(argc + 20).value;
    int b = lazy(99).value;
    printf("static_init order=%d,%d,%d first=%d second=%d derived=%d lazy=%d,%d t=%d\n", order[0], order[1],
           order_n, first.value, second.value, derived_global, a, b, table.v[argc + 4]);
    return 0;
}
