#include <stdint.h>
#include <stdio.h>
#include <stdarg.h>
#include <math.h>
#include <string.h>
#include <pthread.h>

typedef struct { int64_t a; double b; } Pair;
extern int64_t checked_add(int64_t,int64_t,uint8_t *);
extern double float_math(double,double);
extern int64_t float_cast(double);
extern int32_t float_unordered(double,double);
extern Pair make_pair(int64_t,double),call_host_make_pair(int64_t,double);
extern Pair call_pair_callback(Pair (*)(int64_t,double),int64_t,double);
extern double sum_pair(Pair),call_host_pair(Pair),call_variadic(void);
extern int64_t sum_varargs(int32_t,...);
extern int64_t atomic_update(int64_t *,int64_t),atomic_read(int64_t *);
extern int32_t atomic_compare(int64_t *,int64_t,int64_t);
extern void atomic_write(int64_t *,int64_t),copy_bytes(void *,const void *,int64_t);
extern int64_t asm_identity(int64_t),classify(int64_t);
extern int32_t vector_sum(int32_t);
extern int64_t tls_increment(void);
extern _Thread_local int64_t tls_counter;
extern const struct { const char *ptr; int64_t length; } text_view;
static void *tls_worker(void *output) {
    *(int *)output=(tls_counter==0 && tls_increment()==1 && tls_increment()==2);
    return NULL;
}

double host_pair(Pair p) { return (double)p.a-p.b; }
Pair host_make_pair(int64_t a,double b) { Pair p={a+1,b+0.5}; return p; }
double host_variadic(int32_t n,...) {
    va_list list; va_start(list,n); int x=va_arg(list,int); double y=va_arg(list,double); va_end(list);
    return n+x+y;
}
int main(void) {
    uint8_t overflow=9;
    if (checked_add(INT64_MAX,1,&overflow)!=INT64_MIN || overflow!=1) return 1;
    if (checked_add(-10,4,&overflow)!=-6 || overflow!=0) return 2;
    if (float_math(1.25,-3.5)!=6.0 || float_cast(-3.75)!=-3) return 3;
    if (float_cast(NAN)!=0 || float_unordered(NAN,1.0)!=1 || float_unordered(1.0,2.0)!=0) return 4;
    Pair p=make_pair(7,2.5); if (p.a!=7 || p.b!=2.5 || sum_pair(p)!=9.5 || call_host_pair(p)!=4.5) return 5;
    p=call_host_make_pair(8,4.0); if (p.a!=9 || p.b!=4.5) return 6;
    p=call_pair_callback(host_make_pair,10,1.25); if (p.a!=11 || p.b!=1.75) return 7;
    if (call_variadic()!=8.5 || sum_varargs(0)!=0 || sum_varargs(3,INT64_C(1),INT64_C(-3),INT64_C(7))!=5) return 8;
    if (sum_varargs(10,INT64_C(1),INT64_C(2),INT64_C(3),INT64_C(4),INT64_C(5),INT64_C(6),INT64_C(7),INT64_C(8),INT64_C(9),INT64_C(10))!=55) return 9;
    int64_t cell=4; if (atomic_update(&cell,3)!=4 || cell!=7) return 10;
    if (atomic_compare(&cell,7,12)!=1 || atomic_compare(&cell,7,13)!=0 || atomic_read(&cell)!=12) return 11;
    atomic_write(&cell,9); if (cell!=9) return 12;
    const char text[]="Keel record ABI"; char copied[sizeof(text)]={0}; copy_bytes(copied,text,sizeof(text));
    if (memcmp(text,copied,sizeof(text)) || asm_identity(-111)!=-111 || vector_sum(3)!=24) return 13;
    if (classify(1)!=10 || classify(3)!=30 || classify(2)!=-1) return 14;
    if (text_view.length!=5 || memcmp(text_view.ptr,"hello",5)) return 15;
    if (tls_counter!=0 || tls_increment()!=1 || tls_increment()!=2) return 16;
    pthread_t thread; int worker_ok=0;
    if (pthread_create(&thread,NULL,tls_worker,&worker_ok) || pthread_join(thread,NULL) || !worker_ok || tls_counter!=2) return 17;
    puts("compiler API passed"); return 0;
}
