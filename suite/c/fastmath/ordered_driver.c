/* Independent strict left fold, guard-page checks and a common timing driver.
 * Compile this file without fast math and link kernels separately, without LTO. */
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>
extern double sum64(const double *, unsigned long);
extern double abs64(const double *, unsigned long);
extern float sum32(const float *, unsigned long);
extern float abs32(const float *, unsigned long);
static volatile double sink;
static double reference64(const double *p, unsigned long n, int absolute) {
  volatile double s = 0;
  for (unsigned long i = 0; i < n; ++i) s = s + (absolute ? fabs(p[i]) : p[i]);
  return s;
}
static float reference32(const float *p, unsigned long n, int absolute) {
  volatile float s = 0;
  for (unsigned long i = 0; i < n; ++i) s = s + (absolute ? fabsf(p[i]) : p[i]);
  return s;
}
static void equal64(double a, double b) {
  if (!(isnan(a) && isnan(b)) && memcmp(&a, &b, sizeof(a))) {
    fprintf(stderr, "ordered f64 mismatch: %a != %a\n", a, b); exit(1);
  }
}
static void equal32(float a, float b) {
  if (!(isnan(a) && isnan(b)) && memcmp(&a, &b, sizeof(a))) {
    fprintf(stderr, "ordered f32 mismatch: %a != %a\n", a, b); exit(1);
  }
}
static void check(void) {
  size_t page = (size_t)sysconf(_SC_PAGESIZE);
  char *mem = mmap(0, page * 2, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANON, -1, 0);
  if (mem == MAP_FAILED || mprotect(mem + page, page, PROT_NONE)) exit(2);
  const unsigned long sizes[] = {0,1,7,8,9,15,16,17,31,32,33};
  const double dpattern[] = {0.,-0.,9007199254740992.,1.,-9007199254740992.,-3.5};
  const float fpattern[] = {0.f,-0.f,16777216.f,1.f,-16777216.f,-3.5f};
  for (unsigned k = 0; k < sizeof(sizes)/sizeof(sizes[0]); ++k) {
    unsigned long n = sizes[k];
    for (int mode = 0; mode < 6; ++mode) {
      double *d = (double *)(mem + page) - n;
      for (unsigned long i = 0; i < n; ++i) d[i] = dpattern[i % 6];
      if (n && mode) d[n/2] = mode==1 ? INFINITY : mode==2 ? -INFINITY : mode==3 ? NAN : mode==4 ? 0x1p-1074 : -0.;
      equal64(sum64(d,n),reference64(d,n,0));
      equal64(abs64(d,n),reference64(d,n,1));
      float *f = (float *)(mem + page) - n;
      for (unsigned long i = 0; i < n; ++i) f[i] = fpattern[i % 6];
      if (n && mode) f[n/2] = mode==1 ? INFINITY : mode==2 ? -INFINITY : mode==3 ? NAN : mode==4 ? 0x1p-149f : -0.f;
      equal32(sum32(f,n),reference32(f,n,0));
      equal32(abs32(f,n),reference32(f,n,1));
    }
  }
  if (munmap(mem,page*2)) exit(2);
}
int main(int argc, char **argv) {
  if (argc==2 && !strcmp(argv[1],"check")) { check(); puts("ok"); return 0; }
  if (argc!=4) return 2;
  int kernel=atoi(argv[1]); unsigned long n=strtoul(argv[2],0,10), repeats=strtoul(argv[3],0,10);
  if (kernel<0 || kernel>3 || !n || n>1048576 || !repeats || repeats>100000000) return 2;
  double *d=malloc(n*sizeof(*d)); float *f=malloc(n*sizeof(*f));
  if (!d || !f) return 2;
  uint32_t state=17;
  for (unsigned long i=0;i<n;++i) { state=state*1664525u+1013904223u; d[i]=((int)(state%20001)-10000)/16.; f[i]=(float)d[i]; }
  double expected=kernel<2 ? reference64(d,n,kernel==1) : reference32(f,n,kernel==3);
  double got=kernel==0 ? sum64(d,n) : kernel==1 ? abs64(d,n) : kernel==2 ? sum32(f,n) : abs32(f,n);
  equal64(got,expected);
  double total=0;
  /* Dispatch once, so per-call branch selection cannot dominate short kernels. */
  switch (kernel) {
    case 0: for(unsigned long r=0;r<repeats;++r) total+=sum64(d,n); break;
    case 1: for(unsigned long r=0;r<repeats;++r) total+=abs64(d,n); break;
    case 2: for(unsigned long r=0;r<repeats;++r) total+=sum32(f,n); break;
    case 3: for(unsigned long r=0;r<repeats;++r) total+=abs32(f,n); break;
  }
  sink=total;
  printf("%a\n",total); free(d); free(f); return 0;
}
