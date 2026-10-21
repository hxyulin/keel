typedef struct { unsigned long payload, tag; } Value;
typedef struct Context Context;
struct Context {
 int counter, depth, interrupts, destructors, calls, error;
 int (*interrupt)(Context *);
 void (*destroy)(Context *, unsigned long);
};
static inline int poll(Context *c);
static inline void release(Context *c, Value v);
Value invoke(Context *, Value, int) __attribute__((keel_noninterposable));
int poll_slow(Context *c) {
 c->interrupts++;
 c->counter=4;
 if(poll(c)) return -2;
 return c->interrupt(c);
}
static inline int poll(Context *c) {
 c->counter--;
 if(c->counter<=0) return poll_slow(c);
 return 0;
}
void release_slow(Context *c, Value v) {
 Value scalar={0,0};
 c->destructors++;
 release(c,scalar);
 c->destroy(c,v.payload);
}
static inline void release(Context *c, Value v) {
 if(v.tag==1) {
  int *ref=(int *)v.payload;
  (*ref)--;
  if(*ref==0) release_slow(c,v);
 }
}
Value invoke_slow(Context *c, Value v, int n) {
 c->calls++;
 if(n>0) return invoke(c,v,n-1);
 if(poll(c)) { Value exception={99,3}; c->error++; return exception; }
 return v;
}
Value invoke(Context *c, Value v, int n) { return invoke_slow(c,v,n); }
unsigned long exercise(Context *c, Value v, int n) {
 int interrupted=poll(c);
 release(c,v);
 Value result=invoke(c,v,n);
 return result.payload+result.tag+(unsigned long)(interrupted!=0);
}
