#include "abi.h"
#include <stdio.h>
__thread long tls_value = 7;
long stack_mix(long a,long b,long c,long d,long e,long f,long g,long h,unsigned char i,short j,int k,long l) { return a+b+c+d+e+f+g+h+i+j+k+l; }
__int128 wide_add(long a, __int128 x, long b) { return x+a+b; }
struct pair pair_swap(struct pair x) { struct pair y={x.b,x.a}; return y; }
struct big big_add(struct big x) { x.a+=1; x.b+=2; x.c+=3; x.d+=4; return x; }
struct hfa hfa_swap(struct hfa x) { struct hfa y={x.b,x.a}; return y; }
long hfa_stack(double a,double b,double c,double d,double e,double f,double g,double h,struct hfa x,long tail) { return (long)(a+b+c+d+e+f+g+h+x.a+x.b)+tail; }
long sum_ints(int n, ...) { va_list ap; va_start(ap,n); long s=0; for(int i=0;i<n;i++) s+=va_arg(ap,long); va_end(ap); return s; }
double sum_doubles(int n, double bias, ...) { va_list ap; va_start(ap,bias); double s=bias; for(int i=0;i<n;i++) s+=va_arg(ap,double); va_end(ap); return s; }
__int128 sum_wide(int n, ...) { va_list ap; va_start(ap,n); __int128 s=0; for(int i=0;i<n;i++) s+=va_arg(ap,__int128); va_end(ap); return s; }
static long peek_arg(va_list ap) { return va_arg(ap,long); }
long copy_args(int n, ...) { va_list ap, cp; va_start(ap,n); long peek=peek_arg(ap); va_copy(cp,ap); long a=va_arg(ap,long),b=va_arg(cp,long),c=va_arg(cp,long); va_end(ap); va_end(cp); return peek*1000+a*100+b*10+c; }
int format_args(char *out, const char *fmt, ...) { va_list ap; va_start(ap,fmt); int n=vsnprintf(out,128,fmt,ap); va_end(ap); return n; }
long tls_next(void) { return tls_value++; }
int char_is_unsigned(char x) { return x; }
long aligned_sum(long lead, struct aligned_pair x, long tail) { return lead+x.a+x.b+tail; }
long hfa_boundary(double a,double b,double c,double d,double e,double f,double g,struct hfa x,double tail) { return (long)(a+b+c+d+e+f+g+x.a+x.b+tail); }
