// Loops shaped for auto-vectorization (float/int/reduction/mixed widths) with __restrict.
extern "C" int printf(const char *, ...);

enum { N = 1027 };
static float xs[N], ys[N], out[N];
static int ints[N];
static short narrow[N];
static unsigned char bytes[N];

__attribute__((noinline)) void saxpy(float *__restrict y, const float *__restrict x, float a, int n) {
    for (int i = 0; i < n; i++) y[i] = a * x[i] + y[i];
}
__attribute__((noinline)) long isum(const int *v, int n) {
    long s = 0;
    for (int i = 0; i < n; i++) s += v[i];
    return s;
}
__attribute__((noinline)) int widen(const short *__restrict a, const unsigned char *__restrict b, int n) {
    int s = 0;
    for (int i = 0; i < n; i++) s += a[i] * b[i];
    return s;
}
__attribute__((noinline)) void clampmap(float *__restrict o, const float *__restrict in, int n) {
    for (int i = 0; i < n; i++) {
        float v = in[i] * 0.5f - 3.0f;
        o[i] = v < 0.0f ? 0.0f : (v > 100.0f ? 100.0f : v);
    }
}

int main(int argc, char **) {
    for (int i = 0; i < N; i++) {
        xs[i] = (float)(i % 97) * 0.25f;
        ys[i] = (float)(i % 13);
        ints[i] = (i * 7919) % 1000 - 500;
        narrow[i] = (short)(i % 300 - 150);
        bytes[i] = (unsigned char)(i * 31);
    }
    saxpy(ys, xs, 2.0f + argc, N);
    clampmap(out, ys, N);
    double fsum = 0;
    for (int i = 0; i < N; i++) fsum += out[i];
    printf("vector %ld %d %.3f\n", isum(ints, N), widen(narrow, bytes, N), fsum);
    return 0;
}
