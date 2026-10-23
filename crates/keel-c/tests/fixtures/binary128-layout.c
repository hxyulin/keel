/* Layout and pointer operations only: no extended floating values or ABI. */
typedef __float128 Q;
typedef _Complex float C __attribute__((__mode__(__TC__)));
struct Wrapper { char before; C value; char after; };
union Payload { C values[2]; Q real; };
struct Nested { char before; union Payload payload; char after; };
_Static_assert(sizeof(Q)==16 && _Alignof(Q)==16,"real");
_Static_assert(sizeof(C)==32 && _Alignof(C)==16,"complex");
_Static_assert(sizeof(struct Wrapper)==64,"wrapper size");
_Static_assert(__builtin_offsetof(struct Wrapper,value)==16,"value offset");
_Static_assert(__builtin_offsetof(struct Wrapper,after)==48,"after offset");
_Static_assert(sizeof(struct Nested)==96,"nested size");
_Static_assert(_Alignof(_Complex long double)==16,"x87 complex alignment");
C *tc_element(C *p, int i) { return &p[i]; }
Q *q_element(Q *p, int i) { return &p[i]; }
C *tc_field(struct Wrapper *p) { return &p->value; }
char *tc_after(struct Wrapper *p) { return &p->after; }
void tc_store(struct Wrapper *p, char x) { p->after=x; }
C *tc_nested(struct Nested *p, int i) { return &p->payload.values[i]; }
int tc_layout(void) { return sizeof(C)+_Alignof(C)+sizeof((C[2]){}); }
