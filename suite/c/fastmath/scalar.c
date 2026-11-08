/* Portable input shared by Clang/LLVM and Keel. */
extern float sqrtf(float);
void fm_div(const float *a, const float *b, float *out, unsigned long long n) {
  for (unsigned long long i = 0; i < n; ++i)
    out[i] = a[i] / b[i];
}
void fm_normalize(const float *x, const float *y, const float *z, float *out,
                  unsigned long long n) {
  for (unsigned long long i = 0; i < n; ++i)
    out[i] = x[i] / sqrtf(x[i] * x[i] + y[i] * y[i] + z[i] * z[i]);
}
float fm_dot(const float *a, const float *b, unsigned long long n) {
  float sum = 0;
  for (unsigned long long i = 0; i < n; ++i)
    sum += a[i] * b[i];
  return sum;
}
void fm_rsqrt(const float *x, float *out, unsigned long long n) {
  for (unsigned long long i = 0; i < n; ++i)
    out[i] = 1.0f / sqrtf(x[i]);
}
void fm_length3(const float *x, const float *y, const float *z, float *out,
                unsigned long long n) {
  for (unsigned long long i = 0; i < n; ++i)
    out[i] = sqrtf(x[i] * x[i] + y[i] * y[i] + z[i] * z[i]);
}
void fm_saxpy(const float *x, const float *y, float *out,
              unsigned long long n) {
  for (unsigned long long i = 0; i < n; ++i)
    out[i] = 0.75f * x[i] + y[i];
}
void fm_poly(const float *x, float *out, unsigned long long n) {
  for (unsigned long long i = 0; i < n; ++i)
    out[i] = ((0.125f * x[i] + 0.5f) * x[i] + 0.75f) * x[i] + 1.0f;
}
float fm_sumsq(const float *x, unsigned long long n) {
  float sum = 0;
  for (unsigned long long i = 0; i < n; ++i)
    sum += x[i] * x[i];
  return sum;
}
float fm_distance2(const float *x, const float *y, unsigned long long n) {
  float sum = 0;
  for (unsigned long long i = 0; i < n; ++i) {
    float difference = x[i] - y[i];
    sum += difference * difference;
  }
  return sum;
}
