/* Independent GF(256) algebra, FIPS 197 column-major state; hardware is only
 * the implementation under test. No AES builtins or AES instruction oracle. */
static u8 sb[256], ib[256];
static u8 mul(u8 a, u8 b) {
  u8 r = 0;
  for (int i = 0; i < 8; i++) {
    if (b & 1)
      r ^= a;
    a = (u8)((a << 1) ^ ((a & 128) ? 0x1b : 0));
    b >>= 1;
  }
  return r;
}
static u8 rot(u8 a, int n) { return (u8)((a << n) | (a >> (8 - n))); }
static void init(void) {
  for (int a = 0; a < 256; a++) {
    u8 p = 1;
    for (int i = 0; i < 254; i++)
      p = mul(p, (u8)a);
    sb[a] = p ^ rot(p, 1) ^ rot(p, 2) ^ rot(p, 3) ^ rot(p, 4) ^ 99;
    ib[sb[a]] = (u8)a;
  }
}
static void mix(const u8 *a, u8 *o, int inv) {
  const u8 m[2][4] = {{2, 3, 1, 1}, {14, 11, 13, 9}};
  for (int c = 0; c < 4; c++)
    for (int r = 0; r < 4; r++) {
      u8 z = 0;
      for (int k = 0; k < 4; k++)
        z ^= mul(a[4 * c + k], m[inv][(k - r + 4) % 4]);
      o[4 * c + r] = z;
    }
}
static void ref(int op, const u8 *a, const u8 *b, int control, u8 *o) {
  u8 t[16], u[16];
  if (op == 4) {
    mix(a, o, 1);
    return;
  }
  if (op == 5) {
    for (int j = 0; j < 2; j++) {
      int src = 4 + 8 * j, dst = 8 * j;
      for (int k = 0; k < 4; k++) {
        o[dst + k] = sb[a[src + k]];
        o[dst + 4 + k] = sb[a[src + (k + 1) % 4]];
      }
      o[dst + 4] ^= (u8)control;
    }
    return;
  }
  int inv = op >= 2;
  for (int c = 0; c < 4; c++)
    for (int r = 0; r < 4; r++)
      t[4 * c + r] = (inv ? ib : sb)[a[4 * ((c + (inv ? 4 - r : r)) % 4) + r]];
  if (op == 0 || op == 2) {
    mix(t, u, inv);
    memcpy(t, u, 16);
  }
  for (int k = 0; k < 16; k++)
    o[k] = t[k] ^ b[k];
}
static V vec(const u8 *p) {
  V x;
  memcpy(&x, p, 16);
  return x;
}
static void bytes(u8 *p, V x) { memcpy(p, &x, 16); }
static unsigned csr(void) {
  unsigned x;
  __asm__ volatile("stmxcsr %0" : "=m"(x));
  return x;
}
static int eq(const u8 *a, const u8 *b, int n) { return memcmp(a, b, n) == 0; }
static V run(int op, V a, V b) {
  switch (op) {
  case 0:
    return aes_0_0(a, b);
  case 1:
    return aes_1_0(a, b);
  case 2:
    return aes_2_0(a, b);
  case 3:
    return aes_3_0(a, b);
  case 4:
    return aes_4_0(a);
  default:
    return aes_5_0(a);
  }
}
extern V direct(V, V);
extern V indirect(V, V, V (*)(V, V));
/* Physically clobber the non-return SIMD bank, forcing the caller to preserve
 * inputs. A clobber list alone does not overwrite hardware registers. */
V callback(V a, V b) {
  u8 x[16], y[16];
  bytes(x, a);
  bytes(y, b);
  for (int i = 0; i < 16; i++)
    x[i] ^= y[15 - i];
  V r = vec(x);
  __asm__ volatile("pxor %%xmm1,%%xmm1\n\t"
                   "pxor %%xmm2,%%xmm2\n\t"
                   "pxor %%xmm3,%%xmm3\n\t"
                   "pxor %%xmm4,%%xmm4\n\t"
                   "pxor %%xmm5,%%xmm5\n\t"
                   "pxor %%xmm6,%%xmm6\n\t"
                   "pxor %%xmm7,%%xmm7\n\t"
                   "pxor %%xmm8,%%xmm8\n\t"
                   "pxor %%xmm9,%%xmm9\n\t"
                   "pxor %%xmm10,%%xmm10\n\t"
                   "pxor %%xmm11,%%xmm11\n\t"
                   "pxor %%xmm12,%%xmm12\n\t"
                   "pxor %%xmm13,%%xmm13\n\t"
                   "pxor %%xmm14,%%xmm14\n\t"
                   "pxor %%xmm15,%%xmm15" ::
                       : "xmm1", "xmm2", "xmm3", "xmm4", "xmm5", "xmm6", "xmm7",
                         "xmm8", "xmm9", "xmm10", "xmm11", "xmm12", "xmm13",
                         "xmm14", "xmm15");
  return r;
}
static void (*live[6][2])(const void *, const void *, void *) = {
    {live_0_0, live_0_1}, {live_1_0, live_1_1}, {live_2_0, live_2_1},
    {live_3_0, live_3_1}, {live_4_0, live_4_1}, {live_5_0, live_5_1}};
static void (*memops[6])(const void *, const void *, void *) = {
    aesmem_0, aesmem_1, aesmem_2, aesmem_3, aesmem_4, aesmem_5};
static int check(const u8 *a, const u8 *b) {
  u8 expected[16], out[64], z[16];
  unsigned before = csr();
  for (int op = 0; op < 6; op++) {
    ref(op, a, b, 0, expected);
    bytes(out, run(op, vec(a), vec(b)));
    if (!eq(out, expected, 16)) {
      printf("round %d\n", op);
      return 1;
    }
    memset(out, 0xa5, 64);
    memops[op](a, b, out);
    if (!eq(out, expected, 16) || out[16] != 0xa5)
      return 7;
    for (int alias = 0; alias < 2; alias++) {
      memset(out, 0xa5, 64);
      ref(op, a, alias ? a : b, 0, expected);
      live[op][alias](a, b, out + 1);
      if (out[0] != 0xa5 || out[49] != 0xa5 || !eq(out + 1, expected, 16) ||
          !eq(out + 17, a, 16) || !eq(out + 33, b, 16)) {
        printf("live %d alias %d\n", op, alias);
        return 2;
      }
    }
  }
  for (int n = 0; n < 256; n++) {
    ref(5, a, b, n, expected);
    bytes(out, kg[n](vec(a)));
    if (!eq(out, expected, 16)) {
      printf("keygen %d\n", n);
      return 3;
    }
  }
  bytes(z, callback(vec(a), vec(b)));
  ref(0, a, z, 0, expected);
  bytes(out, direct(vec(a), vec(b)));
  if (!eq(out, expected, 16))
    return 4;
  ref(2, b, z, 0, expected);
  bytes(out, indirect(vec(a), vec(b), callback));
  if (!eq(out, expected, 16))
    return 5;
  if (csr() != before)
    return 6;
  return 0;
}
/* NIST FIPS 197 (2001), Appendix C.1. Independent key expansion. */
static int fips(void) {
  u8 key[176], p[16], x[16],
      want[16] = {0x69, 0xc4, 0xe0, 0xd8, 0x6a, 0x7b, 0x04, 0x30,
                  0xd8, 0xcd, 0xb7, 0x80, 0x70, 0xb4, 0xc5, 0x5a};
  for (int i = 0; i < 16; i++) {
    key[i] = (u8)i;
    p[i] = (u8)(17 * i);
    x[i] = p[i] ^ key[i];
  }
  u8 rc = 1;
  for (int i = 16; i < 176; i += 4) {
    u8 t[4];
    memcpy(t, key + i - 4, 4);
    if (i % 16 == 0) {
      u8 v = t[0];
      t[0] = sb[t[1]] ^ rc;
      t[1] = sb[t[2]];
      t[2] = sb[t[3]];
      t[3] = sb[v];
      rc = mul(rc, 2);
    }
    for (int j = 0; j < 4; j++)
      key[i + j] = key[i - 16 + j] ^ t[j];
  }
  V a = vec(x);
  for (int r = 1; r < 10; r++)
    a = aes_0_0(a, vec(key + 16 * r));
  a = aes_1_0(a, vec(key + 160));
  bytes(x, a);
  if (!eq(x, want, 16))
    return 10;
  for (int i = 0; i < 16; i++)
    x[i] ^= key[160 + i];
  a = vec(x);
  for (int r = 9; r > 0; r--)
    a = aes_2_0(a, aes_4_0(vec(key + 16 * r)));
  a = aes_3_0(a, vec(key));
  bytes(x, a);
  return eq(x, p, 16) ? 0 : 11;
}
int main(void) {
  __builtin_cpu_init();
  if (!__builtin_cpu_supports("aes") || !__builtin_cpu_supports("sse2")) {
    puts("SKIP AES host");
    return 77;
  }
  init();
  if (sb[0] != 0x63 || sb[0x53] != 0xed)
    return 20;
  int e = fips();
  if (e)
    return e;
  u8 a[16], b[16];
  unsigned rng = 0x9e3779b9;
  for (int trial = 0; trial < 260; trial++) {
    for (int j = 0; j < 16; j++) {
      rng ^= rng << 13;
      rng ^= rng >> 17;
      rng ^= rng << 5;
      a[j] = (u8)rng;
      b[j] = (u8)(rng >> 8);
    }
    if (trial < 128) {
      memset(a, 0, 16);
      memset(b, 0, 16);
      a[trial / 8] = (u8)(1u << (trial % 8));
      b[(127 - trial) / 8] = (u8)(1u << ((127 - trial) % 8));
    }
    if (trial == 128)
      memset(a, 0, 16);
    if (trial == 129)
      memset(a, 255, 16);
    e = check(a, b);
    if (e) {
      printf("case %d error %d\n", trial, e);
      return e;
    }
  }
  long page = sysconf(_SC_PAGESIZE);
  u8 *m =
      mmap(0, (size_t)page * 3, PROT_NONE, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
  if (m == MAP_FAILED)
    return 30;
  if (mprotect(m + page, (size_t)page, PROT_READ | PROT_WRITE))
    return 31;
  /* Register intrinsics consume explicit align-1 loads. Folding these into a
   * legacy AES m128 source would introduce an alignment fault. Test each input. */
  memcpy(m + page + 1, a, 16);
  for (int input = 0; input < 2; input++) {
    const u8 *x = input ? a : m + page + 1;
    const u8 *y = input ? m + page + 1 : b;
    for (int op = 0; op < 6; op++) {
      u8 expected[16], out[48];
      ref(op, x, y, 0, expected);
      live[op][0](x, y, out);
      if (!eq(out, expected, 16) || !eq(out + 16, x, 16) ||
          !eq(out + 32, y, 16))
        return 32;
    }
  }
  for (int edge = 0; edge < 2; edge++) {
    u8 *p = m + page + (edge ? page - 16 : 0);
    memcpy(p, a, 16);
    e = check(p, b);
    if (e)
      return e;
    e = check(b, p);
    if (e)
      return e;
    for (int op = 0; op < 6; op++) {
      u8 *out = m + page + (edge ? page - 48 : 0);
      live[op][0](a, b, out);
      memops[op](a, b, m + page + (edge ? page - 16 : 0));
    }
  }
  munmap(m, (size_t)page * 3);
  puts("PASS independent AES oracle: six ops, 256 controls, basis/random, FIPS "
       "encrypt/decrypt, ABI/live/aliases/MXCSR/guards");
  return 0;
}
