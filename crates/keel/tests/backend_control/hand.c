typedef __UINT64_TYPE__ U;
extern U callback(U);
U hand_cold_call(U x, unsigned flag) {
  U r = x * 26 + 10;
  return flag ? r + callback(x) : r;
}
U hand_pressure(const U *p, unsigned n) {
  U sum = 0, b = 2, d = 4, f = 6, h = 8;
  for (unsigned i = 0; i < n; i++) {
    U t = p[i];
    sum += t;
    b ^= t;
    d ^= t * 5;
    f ^= t * 11;
    h ^= t * 17;
  }
  return 16 + sum * 24 + b + d + f + h;
}
U hand_branch_loop(const U *p, unsigned n, U mask) {
  U x = 0;
  for (unsigned i = 0; i < n; i++) {
    U t = p[i];
    U a = x + t, b = x ^ (t + 1);
    x = (t & mask) ? a : b;
  }
  return x;
}

U hand_dispatch(const unsigned char *op, const U *data, unsigned n) {
  U x = 1, y = 7;
  if (!n)
    return x ^ y;
  do {
    unsigned o = *op++ & 7;
    switch (o) {
    case 0:
      x += *data;
      break;
    case 1:
      y ^= *data;
      break;
    case 2:
      x ^= y + *data;
      break;
    case 3:
      y += x ^ *data;
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
    ++data;
  } while (--n);
  return x ^ y;
}
