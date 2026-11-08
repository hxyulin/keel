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
