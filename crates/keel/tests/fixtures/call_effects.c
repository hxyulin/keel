/* Bounded no-memory proofs must preserve calls, effects and original reads. */
static __attribute__((noinline)) long scalar(long x) { return x + x; }
static __attribute__((noinline)) long layer1(long x) { return scalar(x) + 1; }
static __attribute__((noinline)) long layer2(long x) { return layer1(x) + 2; }
static __attribute__((noinline)) long write_leaf(long *p, long x) { *p = x; return x + 1; }
static __attribute__((noinline)) long write_layer(long *p, long x) { return write_leaf(p, x) + 2; }
static __attribute__((noinline)) long callback_leaf(long *p, long x, long (*cb)(long *, long)) { return cb(p, x); }
static __attribute__((noinline)) long callback_layer(long *p, long x, long (*cb)(long *, long)) { return callback_leaf(p, x, cb); }
long effects_reuse(long *p, long x) { long a = *p; long b = layer2(x); return a + b + *p; }
long effects_alias(long *p, long *q, long x) { long a = *p; long b = write_layer(q, x); return a + b + *p; }
long effects_callback(long *p, long x, long (*cb)(long *, long)) { long a = *p; long b = callback_layer(p, x, cb); return a + b + *p; }
long effects_order(long *p, long *marker, long x) { long a = *p; long b = write_layer(marker, x); return a + b + *p; }
long effects_spill(long *p, long x) {
 long a0 = p[0];
 long a1 = p[1];
 long a2 = p[2];
 long a3 = p[3];
 long a4 = p[4];
 long a5 = p[5];
 long a6 = p[6];
 long a7 = p[7];
 long a8 = p[8];
 long a9 = p[9];
 long a10 = p[10];
 long a11 = p[11];
 long a12 = p[12];
 long a13 = p[13];
 long a14 = p[14];
 long a15 = p[15];
 long a16 = p[16];
 long a17 = p[17];
 long a18 = p[18];
 long a19 = p[19];
 long a20 = p[20];
 long a21 = p[21];
 long a22 = p[22];
 long a23 = p[23];
 long a24 = p[24];
 long a25 = p[25];
 long a26 = p[26];
 long a27 = p[27];
 long a28 = p[28];
 long a29 = p[29];
 long a30 = p[30];
 long a31 = p[31];
 long a32 = p[32];
 long a33 = p[33];
 long a34 = p[34];
 long a35 = p[35];
 long a36 = p[36];
 long a37 = p[37];
 long a38 = p[38];
 long a39 = p[39];
 long b = layer2(x);
 return b + a0 + a1 + a2 + a3 + a4 + a5 + a6 + a7 + a8 + a9 + a10 + a11 + a12 + a13 + a14 + a15 + a16 + a17 + a18 + a19 + a20 + a21 + a22 + a23 + a24 + a25 + a26 + a27 + a28 + a29 + a30 + a31 + a32 + a33 + a34 + a35 + a36 + a37 + a38 + a39 + p[0];
}
