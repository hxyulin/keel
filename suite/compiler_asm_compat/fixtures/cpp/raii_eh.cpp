// RAII destructors during unwinding, throw/catch by type, rethrow, nested try, noexcept.
extern "C" int printf(const char *, ...);

static int log_pos = 0;
static char log_buf[128];
static void note(char c) { if (log_pos < 127) log_buf[log_pos++] = c; }

struct Guard {
    char c;
    explicit Guard(char c) : c(c) { note(c); }
    ~Guard() { note(c - 'a' + 'A'); }
};
struct Error { int code; };
struct Derived : Error { Derived(int c) : Error{c} {} };

__attribute__((noinline)) int leaf(int x) {
    Guard g('c');
    if (x % 3 == 0) throw Derived{x};
    if (x % 3 == 1) throw 7L;
    return x;
}
__attribute__((noinline)) int middle(int x) {
    Guard g('b');
    try {
        return leaf(x);
    } catch (long v) {
        note('L');
        return (int)v * 100;
    }
}
__attribute__((noinline)) int top(int x) {
    Guard g('a');
    try {
        try {
            return middle(x);
        } catch (Error &e) {
            note('E');
            if (e.code > 5) throw;
            return -e.code;
        }
    } catch (Derived &d) {
        note('D');
        return d.code * 1000;
    }
}
static int never_throws(int x) noexcept { return x * 2; }

int main(int argc, char **) {
    long acc = 0;
    for (int i = argc - 1; i < 9; i++) acc = acc * 13 + top(i) + never_throws(i);
    log_buf[log_pos] = 0;
    printf("raii_eh %ld %s\n", acc, log_buf);
    return 0;
}
