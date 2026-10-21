#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
extern bool bool_not(bool),bool_direct(bool),bool_indirect(bool (*)(bool),bool);
extern bool bool_many(bool (*)(bool,bool,bool,bool,bool,bool,bool,bool,bool,bool),bool,bool);
extern bool bool_stack(bool,bool,bool,bool,bool,bool,bool,bool,bool,bool);
bool external_bool(bool x) { return !x; }
static bool callback(bool a,bool b,bool c,bool d,bool e,bool f,bool g,bool h,bool i,bool j) {
    return a != b != c != d != e != f != g != h != i != j;
}
#define CHECK(x) do { if (!(x)) abort(); } while(0)
int main(void) {
    for (unsigned x=0; x<1024; ++x) {
        bool a=x&1,b=x&2,c=x&4,d=x&8,e=x&16,f=x&32,g=x&64,h=x&128,i=x&256,j=x&512;
        CHECK(bool_not(a)==!a);
        CHECK(bool_direct(a)==!a);
        CHECK(bool_indirect(external_bool,a)==!a);
        CHECK(bool_indirect(bool_not,a)==!a);
        CHECK(bool_many(callback,a,b)==(a!=b));
        CHECK(bool_stack(a,b,c,d,e,f,g,h,i,j)==callback(a,b,c,d,e,f,g,h,i,j));
    }
    puts("1024 boolean patterns passed");
}
