#include <math.h>
#include <string.h>
void fused32(const float *, const float *, const float *, float *);
void fused64(const double *, const double *, const double *, double *);
int main(void) {
  const float a[] = {0x1.000002p0f, 0.1f, 0x1p100f, 0x1p-126f};
  const float b[] = {0x1.fffffep-1f, 10.0f, 0x1p-100f, 0.5f};
  const float c[] = {-1.0f, -1.0f, -1.0f, -0x1p-127f};
  for (unsigned alias = 0; alias < 4; ++alias) {
    float x[4], y[4], z[4], out[4], expected[4];
    memcpy(x, a, sizeof x);
    memcpy(y, b, sizeof y);
    memcpy(z, c, sizeof z);
    for (unsigned i = 0; i < 4; ++i)
      expected[i] = fmaf(x[i], y[i], z[i]);
    float *d = alias == 0 ? out : alias == 1 ? x : alias == 2 ? y : z;
    fused32(x, y, z, d);
    if (memcmp(d, expected, sizeof expected))
      return 1;
  }
  const double da[] = {0x1.0000000000001p0, 0.1};
  const double db[] = {0x1.fffffffffffffp-1, 10.0};
  const double dc[] = {-1.0, -1.0};
  for (unsigned alias = 0; alias < 4; ++alias) {
    double x[2], y[2], z[2], out[2], expected[2];
    memcpy(x, da, sizeof x);
    memcpy(y, db, sizeof y);
    memcpy(z, dc, sizeof z);
    for (unsigned i = 0; i < 2; ++i)
      expected[i] = fma(x[i], y[i], z[i]);
    double *d = alias == 0 ? out : alias == 1 ? x : alias == 2 ? y : z;
    fused64(x, y, z, d);
    if (memcmp(d, expected, sizeof expected))
      return 2;
  }
  return 0;
}
