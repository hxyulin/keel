#include <cstdio>
#include <stdexcept>
extern "C" {
typedef struct { unsigned long payload, tag; } Value;
typedef struct Context Context;
struct Context {
 int counter, depth, interrupts, destructors, calls, error;
 int (*interrupt)(Context *);
 void (*destroy)(Context *, unsigned long);
};
unsigned long exercise(Context *, Value, int);
static int interrupt_ok(Context *) { return 0; }
static int interrupt_error(Context *) { return -1; }
static int interrupt_throw(Context *) { throw 71; }
static void destroy_ok(Context *c, unsigned long p) { if(*(int *)p!=0) throw 72; c->depth++; }
static void destroy_throw(Context *, unsigned long) { throw 73; }
}
struct Guard { int *p; ~Guard() { ++*p; } };
int main() {
 for(int n=0;n<6;n++) {
  Context c={1,0,0,0,0,0,interrupt_ok,destroy_ok};
  Value scalar={17,0};
  if(exercise(&c,scalar,n)!=17 || c.calls!=n+1 || c.interrupts!=1) return 1;
  int ref=1; Value managed={(unsigned long)&ref,1};
  c.counter=9;
  if(exercise(&c,managed,n)!=(unsigned long)&ref+1 || ref!=0 || c.destructors!=1 || c.depth!=1) return 2;
 }
 Context c={2,0,0,0,0,0,interrupt_error,destroy_ok};
 Value scalar={17,0};
 if(exercise(&c,scalar,2)!=102 || c.error!=1 || c.interrupts!=1) return 3;
 int destroyed=0;
 c.counter=0; c.interrupt=interrupt_throw;
 try { Guard g{&destroyed}; exercise(&c,scalar,0); return 4; }
 catch(int x) { if(x!=71 || destroyed!=1) return 5; }
 int ref=1; Value managed={(unsigned long)&ref,1};
 c.counter=9; c.interrupt=interrupt_ok; c.destroy=destroy_throw;
 try { Guard g{&destroyed}; exercise(&c,managed,0); return 6; }
 catch(int x) { if(x!=73 || destroyed!=2 || ref!=0) return 7; }
 std::puts("callbacks, recursive slow calls, byval, exceptions: ok");
}
