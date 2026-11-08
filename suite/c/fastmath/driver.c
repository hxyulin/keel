#define _POSIX_C_SOURCE 200809L
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
void fm_div(const float *, const float *, float *, unsigned long long);
void fm_normalize(const float *, const float *, const float *, float *,
                  unsigned long long);
float fm_dot(const float *, const float *, unsigned long long);
static volatile float sink;
static double now(void) {
  struct timespec t;
  if (clock_gettime(CLOCK_MONOTONIC, &t))
    exit(2);
  return t.tv_sec + t.tv_nsec * 1e-9;
}
static void run(const char *name, float *a, float *b, float *c, float *out,
                unsigned n) {
  if (!strcmp(name, "division"))
    fm_div(a, b, out, n);
  else if (!strcmp(name, "normalize"))
    fm_normalize(a, b, c, out, n);
  else
    out[0] = fm_dot(a, b, n);
  sink = out[0];
}
/* The oracle and timing driver are always compiled without fast-math.
 * Dot error is normalized by sum |a*b|, so cancellation does not hide error
 * behind an arbitrary relative-error denominator. */
static void check(const char *name, float *a, float *b, float *c, float *out,
                  unsigned n, double *absolute, double *scaled) {
  *absolute = *scaled = 0;
  int dot = !strcmp(name, "dot");
  long double sum = 0, magnitude = 0;
  if (dot)
    for (unsigned i = 0; i < n; ++i) {
      long double p = (long double)a[i] * b[i];
      sum += p;
      magnitude += fabsl(p);
    }
  for (unsigned i = 0; i < (dot ? 1 : n); ++i) {
    long double expected =
        dot ? sum
        : !strcmp(name, "division")
            ? (long double)a[i] / b[i]
            : (long double)a[i] /
                  sqrtl((long double)a[i] * a[i] + (long double)b[i] * b[i] +
                        (long double)c[i] * c[i]);
    long double error = fabsl(out[i] - expected);
    long double scale = dot ? magnitude : fabsl(expected);
    double ratio = (double)(error / fmaxl(scale, 1e-30L));
    if (!isfinite(out[i]) || ratio > (dot ? 5e-5 : 2e-6)) {
      fprintf(
          stderr,
          "%s failed: n=%u lane=%u got=%.9g expected=%.18Lg scaled_error=%g\n",
          name, n, i, out[i], expected, ratio);
      exit(1);
    }
    if (error > *absolute)
      *absolute = (double)error;
    if (ratio > *scaled)
      *scaled = ratio;
  }
}
int main(int argc, char **argv) {
  unsigned n = argc > 1 ? (unsigned)strtoul(argv[1], 0, 10) : 4099;
  unsigned iterations = argc > 2 ? (unsigned)strtoul(argv[2], 0, 10) : 1000;
  if (!n || n > 1048576 || !iterations || iterations > 1000000)
    return 2;
  float *a = malloc(n * sizeof(float)), *b = malloc(n * sizeof(float));
  float *c = malloc(n * sizeof(float)), *out = malloc(n * sizeof(float));
  if (!a || !b || !c || !out)
    return 2;
  const char *names[] = {"division", "normalize", "dot"};
  /* Exact binary fractions, nonzero normal denominators, mixed signs.
   * Integer permutation keeps setup independent of floating-point modes. */
  for (unsigned i = 0; i < n; ++i) {
    a[i] = (0.25f + ((i * 37u) % 1024) / 256.0f) * (i % 3 ? 1 : -1);
    b[i] = (0.25f + ((i * 73u + 11) % 1024) / 256.0f) * (i % 5 ? 1 : -1);
    c[i] = 0.25f + ((i * 19u + 7) % 1024) / 256.0f;
  }
  for (unsigned k = 0; k < 3; ++k) {
    double absolute, scaled;
    run(names[k], a, b, c, out, n);
    check(names[k], a, b, c, out, n, &absolute, &scaled);
    for (unsigned j = 0; j < 5; ++j)
      run(names[k], a, b, c, out, n);
    double start = now();
    for (unsigned j = 0; j < iterations; ++j)
      run(names[k], a, b, c, out, n);
    double seconds = now() - start;
    check(names[k], a, b, c, out, n, &absolute, &scaled);
    printf("{\"kernel\":\"%s\",\"n\":%u,\"iterations\":%u,\"seconds\":%.9g,"
           "\"max_absolute_error\":%.9g,\"max_scaled_error\":%.9g}\n",
           names[k], n, iterations, seconds, absolute, scaled);
  }
  free(a);
  free(b);
  free(c);
  free(out);
  return 0;
}
