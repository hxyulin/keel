/* Strict input for Keel batching and Clang. No headers or target intrinsics. */
extern double fabs(double);
extern float fabsf(float);
double sum64(const double *p, unsigned long n) {
  double s = 0;
  for (unsigned long i = 0; i < n; ++i) s += p[i];
  return s;
}
double abs64(const double *p, unsigned long n) {
  double s = 0;
  for (unsigned long i = 0; i < n; ++i) s += fabs(p[i]);
  return s;
}
float sum32(const float *p, unsigned long n) {
  float s = 0;
  for (unsigned long i = 0; i < n; ++i) s += p[i];
  return s;
}
float abs32(const float *p, unsigned long n) {
  float s = 0;
  for (unsigned long i = 0; i < n; ++i) s += fabsf(p[i]);
  return s;
}
