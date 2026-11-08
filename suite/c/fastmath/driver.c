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
void fm_rsqrt(const float *, float *, unsigned long long);
void fm_length3(const float *, const float *, const float *, float *,
                unsigned long long);
void fm_saxpy(const float *, const float *, float *, unsigned long long);
void fm_poly(const float *, float *, unsigned long long);
float fm_sumsq(const float *, unsigned long long);
float fm_distance2(const float *, const float *, unsigned long long);
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
  else if (!strcmp(name, "dot"))
    out[0] = fm_dot(a, b, n);
  else if (!strcmp(name, "sumsq"))
    out[0] = fm_sumsq(a, n);
  else if (!strcmp(name, "distance2"))
    out[0] = fm_distance2(a, b, n);
  else if (!strcmp(name, "rsqrt"))
    fm_rsqrt(c, out, n);
  else if (!strcmp(name, "length3"))
    fm_length3(a, b, c, out, n);
  else if (!strcmp(name, "saxpy"))
    fm_saxpy(a, b, out, n);
  else if (!strcmp(name, "polynomial"))
    fm_poly(a, out, n);
  else
    exit(2);
  sink = out[0];
}
/* The oracle and timing driver are always compiled without fast-math.
 * Reduction error is normalized by the sum of absolute terms. Positive sums
 * allow 5e-4 for sequential float accumulation over the long arrays; dot uses
 * 5e-5. Dot normalization ensures cancellation does not hide error
 * behind an arbitrary relative-error denominator. SAXPY and polynomial errors
 * use the sum of absolute terms for the same reason. */
static void check(const char *name, float *a, float *b, float *c, float *out,
                  unsigned n, double *absolute, double *scaled) {
  *absolute = *scaled = 0;
  int dot = !strcmp(name, "dot");
  int sumsq = !strcmp(name, "sumsq");
  int distance2 = !strcmp(name, "distance2");
  int reduction = dot || sumsq || distance2;
  long double sum = 0, magnitude = 0;
  if (reduction)
    for (unsigned i = 0; i < n; ++i) {
      long double x = a[i], y = b[i];
      long double p = dot ? x * y : sumsq ? x * x : (x - y) * (x - y);
      sum += p;
      magnitude += fabsl(p);
    }
  for (unsigned i = 0; i < (reduction ? 1 : n); ++i) {
    long double x = a[i], y = b[i], z = c[i];
    long double expected, scale;
    if (reduction) {
      expected = sum;
      scale = magnitude;
    } else {
      if (!strcmp(name, "division"))
        expected = x / y;
      else if (!strcmp(name, "normalize"))
        expected = x / sqrtl(x * x + y * y + z * z);
      else if (!strcmp(name, "sumsq"))
        out[0] = fm_sumsq(a, n);
      else if (!strcmp(name, "distance2"))
        out[0] = fm_distance2(a, b, n);
      else if (!strcmp(name, "rsqrt"))
        expected = 1 / sqrtl(z);
      else if (!strcmp(name, "length3"))
        expected = sqrtl(x * x + y * y + z * z);
      else if (!strcmp(name, "saxpy"))
        expected = 0.75L * x + y;
      else if (!strcmp(name, "polynomial"))
        expected = ((0.125L * x + 0.5L) * x + 0.75L) * x + 1;
      else
        exit(2);
      scale = fabsl(expected);
      if (!strcmp(name, "saxpy"))
        scale = 0.75L * fabsl(x) + fabsl(y);
      if (!strcmp(name, "polynomial")) {
        long double ax = fabsl(x);
        scale = ((0.125L * ax + 0.5L) * ax + 0.75L) * ax + 1;
      }
    }
    long double error = fabsl(out[i] - expected);
    double ratio = (double)(error / fmaxl(scale, 1e-30L));
    if (!isfinite(out[i]) || ratio > (dot ? 5e-5 : reduction ? 5e-4 : 2e-6)) {
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
  const char *names[] = {"division",   "normalize", "dot",
                         "rsqrt",      "length3",   "saxpy",
                         "polynomial", "sumsq",     "distance2"};
  /* Exact binary fractions, nonzero normal denominators, mixed signs.
   * Integer permutation keeps setup independent of floating-point modes. */
  for (unsigned i = 0; i < n; ++i) {
    a[i] = (0.25f + ((i * 37u) % 1024) / 256.0f) * (i % 3 ? 1 : -1);
    b[i] = (0.25f + ((i * 73u + 11) % 1024) / 256.0f) * (i % 5 ? 1 : -1);
    c[i] = 0.25f + ((i * 19u + 7) % 1024) / 256.0f;
  }
  for (unsigned k = 0; k < sizeof(names) / sizeof(names[0]); ++k) {
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
