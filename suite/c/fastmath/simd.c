/* Four-lane handwritten SIMD. No fast-math flag changes the strict path.
 * FAST_APPROX selects estimates/refinement and a reassociated reduction. */
#include <math.h>
#if defined(__aarch64__)
#include <arm_neon.h>
typedef float32x4_t V;
#define load vld1q_f32
#define store vst1q_f32
#define add vaddq_f32
#define sub vsubq_f32
#define mul vmulq_f32
#define divide vdivq_f32
#define root vsqrtq_f32
#define zero() vdupq_n_f32(0)
#define splat vdupq_n_f32
static V reciprocal(V x) {
  V r = vrecpeq_f32(x);
  r = mul(r, vrecpsq_f32(x, r));
  return mul(r, vrecpsq_f32(x, r));
}
static V invroot(V x) {
  V r = vrsqrteq_f32(x);
  r = mul(r, vrsqrtsq_f32(mul(x, r), r));
  return mul(r, vrsqrtsq_f32(mul(x, r), r));
}
#elif defined(__x86_64__)
#include <xmmintrin.h>
typedef __m128 V;
#define load _mm_loadu_ps
#define store _mm_storeu_ps
#define add _mm_add_ps
#define sub _mm_sub_ps
#define mul _mm_mul_ps
#define divide _mm_div_ps
#define root _mm_sqrt_ps
#define zero() _mm_setzero_ps()
#define splat _mm_set1_ps
static V reciprocal(V x) {
  V r = _mm_rcp_ps(x);
  return mul(r, _mm_sub_ps(_mm_set1_ps(2), mul(x, r)));
}
static V invroot(V x) {
  V r = _mm_rsqrt_ps(x);
  return mul(mul(_mm_set1_ps(0.5f), r),
             _mm_sub_ps(_mm_set1_ps(3), mul(mul(x, r), r)));
}
#else
#error Unsupported SIMD target
#endif
void fm_div(const float *a, const float *b, float *out, unsigned long long n) {
  unsigned long long i = 0;
  for (; i + 4 <= n; i += 4) {
#ifdef FAST_APPROX
    V r = mul(load(a + i), reciprocal(load(b + i)));
#else
    V r = divide(load(a + i), load(b + i));
#endif
    store(out + i, r);
  }
  for (; i < n; ++i)
    out[i] = a[i] / b[i];
}
void fm_normalize(const float *x, const float *y, const float *z, float *out,
                  unsigned long long n) {
  unsigned long long i = 0;
  for (; i + 4 <= n; i += 4) {
    V a = load(x + i), b = load(y + i), c = load(z + i);
    V s = add(add(mul(a, a), mul(b, b)), mul(c, c));
#ifdef FAST_APPROX
    V r = mul(a, invroot(s));
#else
    V r = divide(a, root(s));
#endif
    store(out + i, r);
  }
  for (; i < n; ++i)
    out[i] = x[i] / sqrtf(x[i] * x[i] + y[i] * y[i] + z[i] * z[i]);
}
float fm_dot(const float *a, const float *b, unsigned long long n) {
  unsigned long long i = 0;
  float sum = 0, lanes[4];
#ifdef FAST_APPROX
  V acc0 = zero(), acc1 = zero();
  for (; i + 8 <= n; i += 8) {
    acc0 = add(acc0, mul(load(a + i), load(b + i)));
    acc1 = add(acc1, mul(load(a + i + 4), load(b + i + 4)));
  }
  store(lanes, add(acc0, acc1));
  sum = (lanes[0] + lanes[1]) + (lanes[2] + lanes[3]);
#else
  for (; i + 4 <= n; i += 4) {
    store(lanes, mul(load(a + i), load(b + i)));
    for (unsigned j = 0; j < 4; ++j)
      sum += lanes[j];
  }
#endif
  for (; i < n; ++i)
    sum += a[i] * b[i];
  return sum;
}
void fm_rsqrt(const float *x, float *out, unsigned long long n) {
  unsigned long long i = 0;
  for (; i + 4 <= n; i += 4) {
#ifdef FAST_APPROX
    V r = invroot(load(x + i));
#else
    V r = divide(splat(1), root(load(x + i)));
#endif
    store(out + i, r);
  }
  for (; i < n; ++i)
    out[i] = 1.0f / sqrtf(x[i]);
}
void fm_length3(const float *x, const float *y, const float *z, float *out,
                unsigned long long n) {
  unsigned long long i = 0;
  for (; i + 4 <= n; i += 4) {
    V a = load(x + i), b = load(y + i), c = load(z + i);
    store(out + i, root(add(add(mul(a, a), mul(b, b)), mul(c, c))));
  }
  for (; i < n; ++i)
    out[i] = sqrtf(x[i] * x[i] + y[i] * y[i] + z[i] * z[i]);
}
void fm_saxpy(const float *x, const float *y, float *out,
              unsigned long long n) {
  unsigned long long i = 0;
  for (; i + 4 <= n; i += 4)
    store(out + i, add(mul(splat(0.75f), load(x + i)), load(y + i)));
  for (; i < n; ++i)
    out[i] = 0.75f * x[i] + y[i];
}
void fm_poly(const float *x, float *out, unsigned long long n) {
  unsigned long long i = 0;
  for (; i + 4 <= n; i += 4) {
    V a = load(x + i);
    V r = add(mul(splat(0.125f), a), splat(0.5f));
    r = add(mul(r, a), splat(0.75f));
    store(out + i, add(mul(r, a), splat(1)));
  }
  for (; i < n; ++i)
    out[i] = ((0.125f * x[i] + 0.5f) * x[i] + 0.75f) * x[i] + 1.0f;
}
float fm_sumsq(const float *x, unsigned long long n) {
  unsigned long long i = 0;
  float sum = 0, lanes[4];
#ifdef FAST_APPROX
  V acc0 = zero(), acc1 = zero();
  for (; i + 8 <= n; i += 8) {
    V a = load(x + i), b = load(x + i + 4);
    acc0 = add(acc0, mul(a, a));
    acc1 = add(acc1, mul(b, b));
  }
  store(lanes, add(acc0, acc1));
  sum = (lanes[0] + lanes[1]) + (lanes[2] + lanes[3]);
#else
  for (; i + 4 <= n; i += 4) {
    V a = load(x + i);
    store(lanes, mul(a, a));
    for (unsigned j = 0; j < 4; ++j)
      sum += lanes[j];
  }
#endif
  for (; i < n; ++i)
    sum += x[i] * x[i];
  return sum;
}
float fm_distance2(const float *x, const float *y, unsigned long long n) {
  unsigned long long i = 0;
  float sum = 0, lanes[4];
#ifdef FAST_APPROX
  V acc0 = zero(), acc1 = zero();
  for (; i + 8 <= n; i += 8) {
    V a = sub(load(x + i), load(y + i)),
      b = sub(load(x + i + 4), load(y + i + 4));
    acc0 = add(acc0, mul(a, a));
    acc1 = add(acc1, mul(b, b));
  }
  store(lanes, add(acc0, acc1));
  sum = (lanes[0] + lanes[1]) + (lanes[2] + lanes[3]);
#else
  for (; i + 4 <= n; i += 4) {
    V a = sub(load(x + i), load(y + i));
    store(lanes, mul(a, a));
    for (unsigned j = 0; j < 4; ++j)
      sum += lanes[j];
  }
#endif
  for (; i < n; ++i) {
    float d = x[i] - y[i];
    sum += d * d;
  }
  return sum;
}
