/* By-value records of up to 16 bytes, one per SysV eightbyte class mix. */
typedef struct { double d; long l; } S1;
typedef struct { long l; double d; } S2;
typedef struct { int a, b; double d; } S3;
typedef struct { float a, b; int c; } S4;
typedef struct { float a; int b; } S5;
typedef struct { double a, b; } S6;
typedef struct { char c; double d; } S7;
typedef struct { struct { float x, y; } p; long n; } S8;
typedef struct { float f[2]; double d; } S9;
typedef union { double d; long l; } S10;
typedef struct { union { float f; int i; } u; float g; double d; } S11;
typedef struct { double a, b, c; } S12;
typedef struct { float a; double d; } S13;
typedef struct { double d; float f; } S14;
typedef struct { int i; float f; } S15;
typedef struct { float f; } S16;
typedef struct { short s; float f; } S17;
typedef struct { float a, b, c; } S18;
typedef struct { _Alignas(16) double d; } S19;
typedef struct { double d; int x : 5; int y : 7; } S20;
typedef struct { long a; S6 *p; } S21;
typedef struct { union { double d; float f[2]; } u; long n; } S22;
typedef struct { char c[3]; float f; } S23;
typedef struct { _Alignas(16) long l; } S24;

#define D(k) ((double)(k) + 0.5)
#define F(k) ((float)(k) + 0.25f)
static S1 make_S1(long k) { S1 s = {D(k), 7 + k}; return s; }
static long hash_S1(S1 s) { return (long)(s.d * 16) * 3 + s.l * 5; }
static S2 make_S2(long k) { S2 s = {9 + k, D(k + 1)}; return s; }
static long hash_S2(S2 s) { return (long)(s.d * 16) * 3 + s.l * 5; }
static S3 make_S3(long k) { S3 s = {(int)k + 1, (int)k + 2, D(k + 3)}; return s; }
static long hash_S3(S3 s) { return s.a * 7 + s.b * 11 + (long)(s.d * 16); }
static S4 make_S4(long k) { S4 s = {F(k), F(k + 1), (int)k + 5}; return s; }
static long hash_S4(S4 s) { return (long)(s.a * 8) * 3 + (long)(s.b * 8) * 5 + s.c * 7; }
static S5 make_S5(long k) { S5 s = {F(k + 2), (int)k - 9}; return s; }
static long hash_S5(S5 s) { return (long)(s.a * 8) * 3 + s.b * 13; }
static S6 make_S6(long k) { S6 s = {D(k), D(2 * k)}; return s; }
static long hash_S6(S6 s) { return (long)(s.a * 16) * 3 + (long)(s.b * 16) * 5; }
static S7 make_S7(long k) { S7 s = {(char)(k + 33), D(k + 4)}; return s; }
static long hash_S7(S7 s) { return s.c * 17 + (long)(s.d * 16) * 3; }
static S8 make_S8(long k) { S8 s = {{F(k), F(k + 3)}, 100 + k}; return s; }
static long hash_S8(S8 s) { return (long)(s.p.x * 8) * 3 + (long)(s.p.y * 8) * 5 + s.n * 7; }
static S9 make_S9(long k) { S9 s = {{F(k + 1), F(k + 2)}, D(k + 3)}; return s; }
static long hash_S9(S9 s) { return (long)(s.f[0] * 8) * 3 + (long)(s.f[1] * 8) * 5 + (long)(s.d * 16) * 7; }
static S10 make_S10(long k) { S10 s; s.l = 0x123456789 + k; return s; }
static long hash_S10(S10 s) { return s.l * 3; }
static S11 make_S11(long k) { S11 s; s.u.i = (int)k + 77; s.g = F(k); s.d = D(k); return s; }
static long hash_S11(S11 s) { return s.u.i * 3 + (long)(s.g * 8) * 5 + (long)(s.d * 16) * 7; }
static S12 make_S12(long k) { S12 s = {D(k), D(k + 1), D(k + 2)}; return s; }
static long hash_S12(S12 s) { return (long)(s.a * 16) * 3 + (long)(s.b * 16) * 5 + (long)(s.c * 16) * 7; }
static S13 make_S13(long k) { S13 s = {F(k), D(k + 6)}; return s; }
static long hash_S13(S13 s) { return (long)(s.a * 8) * 3 + (long)(s.d * 16) * 5; }
static S14 make_S14(long k) { S14 s = {D(k + 2), F(k + 8)}; return s; }
static long hash_S14(S14 s) { return (long)(s.d * 16) * 3 + (long)(s.f * 8) * 5; }
static S15 make_S15(long k) { S15 s = {(int)k * 3, F(k)}; return s; }
static long hash_S15(S15 s) { return s.i * 3 + (long)(s.f * 8) * 5; }
static S16 make_S16(long k) { S16 s = {F(k + 11)}; return s; }
static long hash_S16(S16 s) { return (long)(s.f * 8) * 3; }
static S17 make_S17(long k) { S17 s = {(short)(k + 300), F(k)}; return s; }
static long hash_S17(S17 s) { return s.s * 3 + (long)(s.f * 8) * 5; }
static S18 make_S18(long k) { S18 s = {F(k), F(k + 1), F(k + 2)}; return s; }
static long hash_S18(S18 s) { return (long)(s.a * 8) * 3 + (long)(s.b * 8) * 5 + (long)(s.c * 8) * 7; }
static S19 make_S19(long k) { S19 s = {D(k + 5)}; return s; }
static long hash_S19(S19 s) { return (long)(s.d * 16) * 3; }
static S20 make_S20(long k) { S20 s = {D(k), (int)(k % 15) - 7, (int)(k % 60) - 30}; return s; }
static long hash_S20(S20 s) { return (long)(s.d * 16) * 3 + s.x * 5 + s.y * 7; }

static S21 make_S21(long k) { S21 s = {k * 1000 + 1, (S6 *)(k * 16)}; return s; }
static long hash_S21(S21 s) { return s.a * 3 + (long)s.p * 5; }
static S22 make_S22(long k) { S22 s; s.u.d = D(k + 9); s.n = k - 50; return s; }
static long hash_S22(S22 s) { return (long)(s.u.d * 16) * 3 + s.n * 5; }
static S23 make_S23(long k) { S23 s = {{(char)k, (char)(k + 1), (char)(k + 2)}, F(k)}; return s; }
static long hash_S23(S23 s) { return s.c[0] * 3 + s.c[1] * 5 + s.c[2] * 7 + (long)(s.f * 8) * 11; }
static S24 make_S24(long k) { S24 s = {k * 77}; return s; }
static long hash_S24(S24 s) { return s.l * 3; }

#define TYPES(X) X(S1) X(S2) X(S3) X(S4) X(S5) X(S6) X(S7) X(S8) X(S9) X(S10) X(S11) \
  X(S12) X(S13) X(S14) X(S15) X(S16) X(S17) X(S18) X(S19) X(S20) X(S21) X(S22) X(S23) X(S24)

/* mk_ returns the record and take_ receives it. ex_ leaves one SSE and one
 * general register for it, so a record needing two of one class goes on the
 * stack and the trailing double or long still takes the last register.
 * full_ passes it after every argument register is used. */
#define DECL(T)                                                                    \
  T mk_##T(long k);                                                                \
  long take_##T(T s);                                                              \
  long ex_##T(double a0, double a1, double a2, double a3, double a4, double a5,    \
              double a6, long b0, long b1, long b2, long b3, long b4, T s,         \
              double c, long d);                                                   \
  long full_##T(double a0, double a1, double a2, double a3, double a4, double a5,  \
                double a6, double a7, long b0, long b1, long b2, long b3, long b4, \
                long b5, T s, double c, long d);
TYPES(DECL)
