typedef __UINT64_TYPE__ U;
extern U callback(U);
#ifndef PREFIX
#define PREFIX keel_
#endif
#define CAT_(a, b) a##b
#define CAT(a, b) CAT_(a, b)
#define FN(n) CAT(PREFIX, n)
U FN(cold_call)(U x, unsigned flag) {
  U a = x * 3 + 1, b = x * 5 + 2, c = x * 7 + 3, d = x * 11 + 4;
  U r = 0;
  if (flag)
    r = callback(x);
  return a + b + c + d + r;
}
U FN(dispatch)(const unsigned char *op, const U *data, unsigned n) {
  U x = 1, y = 7;
  for (unsigned i = 0; i < n; i++) {
    switch (op[i] & 7) {
    case 0:
      x += data[i];
      break;
    case 1:
      y ^= data[i];
      break;
    case 2:
      x ^= y + data[i];
      break;
    case 3:
      y += x ^ data[i];
      break;
    case 4:
      x = (x << 3) | (x >> 61);
      break;
    case 5:
      y = (y << 7) | (y >> 57);
      break;
    case 6:
      x += y;
      break;
    default:
      y ^= x;
      break;
    }
  }
  return x ^ y;
}
U FN(pressure)(const U *p, unsigned n) {
  U a = 1, b = 2, c = 3, d = 4, e = 5, f = 6, g = 7, h = 8;
  for (unsigned i = 0; i < n; i++) {
    U t = p[i];
    a += t;
    b ^= t;
    c += t * 3;
    d ^= t * 5;
    e += t * 7;
    f ^= t * 11;
    g += t * 13;
    h ^= t * 17;
  }
  return a + b + c + d + e + f + g + h;
}
U FN(branch_loop)(const U *p, unsigned n, U mask) {
  U x = 0;
  for (unsigned i = 0; i < n; i++) {
    U t = p[i];
    if (t & mask)
      x += t;
    else
      x ^= t + 1;
  }
  return x;
}

U FN(across)(const U *p, U seed) {
  U a = p[0], b = p[1], c = p[2], d = p[3], e = p[4], f = p[5];
  U g = p[6], h = p[7], i = p[8], j = p[9], k = p[10], l = p[11];
  U result = callback(seed);
  return a + b + c + d + e + f + g + h + i + j + k + l + result;
}

typedef struct { U payload, tag; } Boxed;
U FN(guarded_values)(const Boxed *, unsigned, unsigned, unsigned, unsigned *);
static void FN(release_slow)(unsigned *effects, Boxed value) {
  ++*effects;
  if (value.tag == 1)
    (void)FN(guarded_values)((const Boxed *)0, 0, 0, 0, effects);
}
static inline void FN(release)(unsigned *effects, Boxed value) {
  if ((unsigned)value.tag >= (unsigned)-11) {
    int *count = (int *)(__UINTPTR_TYPE__)value.payload;
    if (--*count <= 0)
      FN(release_slow)(effects, value);
  }
}
U FN(guarded_values)(const Boxed *values, unsigned n, unsigned depth,
                     unsigned slot, unsigned *effects) {
  volatile unsigned char frame[2048];
  frame[slot & 2047] = (unsigned char)depth;
  U sum = 0;
  for (unsigned i = 0; i < n; ++i) {
    sum += values[i].payload;
    FN(release)(effects, values[i]);
  }
  if (depth)
    sum ^= FN(guarded_values)(values, n, depth - 1, slot + 1, effects);
  return sum + frame[slot & 2047];
}
