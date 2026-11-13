struct narrow c_narrow(int64_t x, void *p) { return (struct narrow){(char)x, p}; }
int64_t call_narrow(int64_t, void *);
struct narrow indirect_narrow(struct narrow (*)(int64_t,void *),int64_t,void *);
struct pair call_fallback(int64_t,int64_t), fallback_caller(int64_t,int64_t);
uint64_t fast_named(uint64_t *,uint64_t), fast_large(uint64_t *,uint64_t);
struct pair wide_pair(int64_t,int64_t,int64_t,int64_t,int64_t,int64_t,int64_t,int64_t,int64_t,int64_t);
struct pair call_wide_pair(int64_t,int64_t,int64_t,int64_t,int64_t,int64_t,int64_t,int64_t,int64_t,int64_t);
int main(void) {
    if (original_main()) return 1;
    uint64_t payload=123;
    uint64_t xs[]={0x123400,0x123480,UINT64_MAX,UINT64_C(0x123456789abcdef0)};
    for (unsigned i=0;i<sizeof(xs)/sizeof(xs[0]);i++) {
        uint64_t x=xs[i];
        CHECK(call_narrow(x,&payload),payload+((uint8_t)x!=0));
        struct narrow n=indirect_narrow(c_narrow,x,&payload);
        CHECK(n.c,(char)x); CHECK(n.p==&payload,1);
        n=indirect_narrow(k_narrow,x,&payload);
        CHECK(n.c,(char)x); CHECK(n.p==&payload,1);
        uint64_t expected=(uint8_t)x+3*(uint64_t)(uint16_t)x+5*(uint64_t)(uint32_t)x+7*x+11*payload+13*(x+19)+17*(uint64_t)(uint32_t)x+19*(uint64_t)(uint8_t)x;
        CHECK(fast_named(&payload,x),expected);
        CHECK(fast_large(&payload,x),expected);
    }
    struct pair p=call_fallback(71,-39); CHECK(p.a,71); CHECK(p.b,-39);
    p=fallback_caller(71,-39); CHECK(p.a,-39); CHECK(p.b,71);
    p=wide_pair(1,2,3,4,5,6,7,8,9,10); CHECK(p.a,10); CHECK(p.b,18);
    p=call_wide_pair(1,2,3,4,5,6,7,8,9,10); CHECK(p.a,18); CHECK(p.b,10);
    return failures!=0;
}
