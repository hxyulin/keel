typedef unsigned int V4 __attribute__((vector_size(16)));
typedef unsigned int V8 __attribute__((vector_size(32)));
void simd_f32(const float *a,float *out,float *live) {float x=*a,y;asm("addss %1,%0":"=x"(y):"0"(x):"xmm4","memory","cc");*out=y;*live=x;}
void simd_f64(const double *a,double *out,double *live) {double x=*a;double y=x;asm("addsd %0,%0":"+v"(y)::"xmm4","memory");*out=y;*live=x;}
void simd_64(const void *a,void *out,void *live) {V4 x={0},y;__builtin_memcpy(&x,a,8);asm("paddd %1,%0":"=&x"(y):"0"(x):"xmm4","memory");__builtin_memcpy(out,&y,8);__builtin_memcpy(live,&x,8);}
void simd_128(const V4 *a,V4 *out,V4 *live) {V4 x=*a,y=x;asm("paddd %0,%0":"+x"(y)::"xmm4","memory");*out=y;*live=x;}
void simd_256(const V8 *a,V8 *out,V8 *live) {V8 x=*a,y=x;asm("vpaddd %0,%0,%0":"+v"(y)::"ymm4","memory");*out=y;*live=x;}
void simd_barrier(const V4 *a,V4 *out,V4 *live) {V4 x=*a,y=x;asm("/* opaque %0 */":"+x"(y)::"xmm4","memory");*out=y;*live=x;}
void simd_clobber(const V8 *a,V8 *out,V8 *live) {V8 x=*a;asm("vpxor %%ymm4,%%ymm4,%%ymm4":::"ymm4","memory");*out=x;*live=x;}
void simd_mixed(const void *a,void *out,void *live) {
    float f,fo;double d,do_;V8 v,vo;unsigned long n,no;
    __builtin_memcpy(&f,a,4);__builtin_memcpy(&d,a,8);
    __builtin_memcpy(&v,a,32);__builtin_memcpy(&n,a,8);
    asm("/*mixed %0 %1 %2 %3*/":"=x"(fo),"=x"(do_),"=v"(vo),"=r"(no):"0"(f),"1"(d),"2"(v),"3"(n):"memory");
    __builtin_memcpy(out,&fo,4);__builtin_memcpy(live,&f,4);
    __builtin_memcpy((char*)out+8,&do_,8);__builtin_memcpy((char*)live+8,&d,8);
    __builtin_memcpy((char*)out+16,&vo,32);__builtin_memcpy((char*)live+16,&v,32);
    __builtin_memcpy((char*)out+48,&no,8);__builtin_memcpy((char*)live+48,&n,8);
}
