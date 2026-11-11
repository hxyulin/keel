/* Strict column reductions. Each output has an ordered, positive-zero fold. */
void col64(const double *a, double *out, unsigned long n) {
  for (unsigned long c=0;c<n;++c) {
    double s=0;
    for (unsigned long r=0;r<n;++r) s+=a[r*n+c];
    out[c]=s;
  }
}
void col32(const float *a, float *out, unsigned long n) {
  for (unsigned long c=0;c<n;++c) {
    float s=0;
    for (unsigned long r=0;r<n;++r) s+=a[r*n+c];
    out[c]=s;
  }
}
