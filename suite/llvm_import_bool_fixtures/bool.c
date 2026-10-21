#include <stdbool.h>
extern bool external_bool(bool);
bool bool_not(bool x) { return !x; }
bool bool_direct(bool x) { return external_bool(x); }
bool bool_indirect(bool (*f)(bool), bool x) { return f(x); }
bool bool_many(bool (*f)(bool,bool,bool,bool,bool,bool,bool,bool,bool,bool), bool a, bool b) {
    return f(a,b,a,b,a,b,a,b,a,b);
}
bool bool_stack(bool a,bool b,bool c,bool d,bool e,bool f,bool g,bool h,bool i,bool j) {
    return (a != b) != (c != d) != (e != f) != (g != h) != (i != j);
}
