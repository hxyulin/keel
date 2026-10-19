/* Driver for kernels.c, compiled once by Clang and linked against each
 * backend's kernels object.
 *   driver check            print every kernel result over edge-case inputs
 *   driver time NAME REPS   time REPS calls in-process (CLOCK_UPTIME_RAW),
 *                           printing "NAME ns checksum"; startup excluded.
 * Inputs come from a fixed xorshift sequence; output is deterministic. */
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

typedef uint64_t u64;
typedef int64_t i64;
typedef uint32_t u32;
typedef int32_t i32;
typedef uint16_t u16;
typedef int16_t i16;
typedef uint8_t u8;
typedef int8_t i8;

extern u64 kernel_calls;
u64 k_logic(const u64 *, u64);
u32 k_fnv1a(const u8 *, u64);
u32 k_crc32(const u8 *, u64);
u64 k_sieve(u8 *, u64);
void k_matmul(const i32 *, const i32 *, i32 *, i32);
double k_dot(const double *, const double *, u64);
void k_saxpy(float, const float *, float *, u64);
i64 k_divmix(const i64 *, const i64 *, u64);
u64 gcd64(u64, u64);
u64 k_gcd(const u64 *, u64);
void k_isort(i32 *, u64);
void k_qsort(i64 *, u64);
i64 k_interp(const u8 *, u64, i64, u64);
i32 k_classify(i32);
i64 k_classify_sum(const i32 *, u64);
u64 k_bits(const u64 *, u64);
i8 clamp_i8(i16);
u16 mix_u16(u8, i8, u16);
i64 k_narrow(const i16 *, u64);
i64 k_fconv(const double *, u64);
u64 k_collatz(u64, u64);
u64 k_copy(u8 *, const u8 *, u64);
u64 k_dispatch(const u64 *, u64);
u64 k_names(u64);
i64 k_stackobj(const i64 *, u64);

static u64 state = 0x9E3779B97F4A7C15ULL;
static u64 rnd(void) {
    state ^= state << 13;
    state ^= state >> 7;
    state ^= state << 17;
    return state;
}
static u64 hash(const void *p, u64 n) {
    const u8 *b = p;
    u64 h = 1469598103934665603ULL;
    for (u64 i = 0; i < n; ++i) h = (h ^ b[i]) * 1099511628211ULL;
    return h;
}
static void show(const char *what, u64 v) { printf("%-16s %016llx\n", what, (unsigned long long)v); }
static u64 dbits(double x) { u64 b; memcpy(&b, &x, 8); return b; }

#define N 4096
static u64 a64[2 * N], b64[N];
static i64 s64[N], d64[N], qs[N];
static u8 bytes[N], flags[1 << 16], dst[N], code[64];
static i32 m1[64 * 64], m2[64 * 64], m3[64 * 64], is[N], cls[N];
static double dx[N], dy[N], fc[N];
static float fx[N], fy[N];
static i16 nar[N];

static void init(void) {
    for (u64 i = 0; i < 2 * N; ++i) a64[i] = rnd();
    for (u64 i = 0; i < N; ++i) {
        u64 r = rnd();
        b64[i] = (i % 17 == 0) ? 0 : (i % 13 == 0) ? ~0ULL : r;
        s64[i] = (i64)(r >> 2) - (i64)(1ULL << 61); /* never INT64_MIN */
        i64 den = (i64)(rnd() % 2001) - 1000;
        d64[i] = den ? den : 1;
        qs[i] = (i64)rnd();
        bytes[i] = (u8)r;
        dst[i] = 0;
        is[i] = (i32)r;
        dx[i] = (double)(i64)(r % 2000001) / 1000.0 - 1000.0;
        dy[i] = (double)(i64)(rnd() % 2000001) / 977.0 - 1000.0;
        fx[i] = (float)dx[i];
        fy[i] = (float)dy[i];
        nar[i] = (i16)r;
        static const i32 special[] = {-1000, -7, 0, 3, 64, 1 << 20, 2147483647, -2147483647 - 1, 1, -1};
        cls[i] = (i % 3 == 0) ? special[(i / 3) % 10] : (i32)r;
    }
    static const double special[] = {0.0, -0.0, NAN, -NAN, INFINITY, -INFINITY, 1e9, -1e9, 3.99e9, 4.2e9,
                                     0.5, -0.5, 1.5, -1.5, 2147483647.0, -2147483648.0, 1e300, -1e-300};
    for (u64 i = 0; i < N; ++i)
        fc[i] = i < sizeof special / sizeof *special ? special[i] : dx[i] * 1.0e6;
    for (u64 i = 0; i < 64; ++i) code[i] = (u8)(rnd() % 9);
    for (u64 i = 0; i < 64 * 64; ++i) {
        m1[i] = (i32)(rnd() % 2001) - 1000;
        m2[i] = (i32)(rnd() % 2001) - 1000;
    }
    i16 edges[] = {-32768, -32767, -129, -128, -127, -1, 0, 1, 126, 127, 128, 255, 256, 32767};
    memcpy(nar, edges, sizeof edges);
}

static int check(void) {
    u64 sizes[] = {0, 1, 2, 7, 64, 1000, N};
    for (unsigned s = 0; s < sizeof sizes / sizeof *sizes; ++s) {
        u64 n = sizes[s];
        char tag[32];
        printf("== n=%llu\n", (unsigned long long)n);
        show("logic", k_logic(a64, n));
        show("fnv1a", k_fnv1a(bytes, n));
        show("crc32", k_crc32(bytes, n));
        show("sieve", k_sieve(flags, n * 16));
        show("sieve-flags", hash(flags, n * 16));
        show("dot", dbits(k_dot(dx, dy, n)));
        memcpy(fy, fx, sizeof fy);
        k_saxpy(1.25f, fx, fy, n);
        show("saxpy", hash(fy, sizeof fy));
        show("divmix", (u64)k_divmix(s64, d64, n));
        show("gcd", k_gcd(b64, n));
        memcpy(is, a64, sizeof is);
        k_isort(is, n < 1000 ? n : 1000);
        show("isort", hash(is, sizeof is));
        for (u64 i = 0; i < N; ++i) qs[i] = (i64)a64[i] >> (i % 5 ? 0 : 40);
        k_qsort(qs, n);
        show("qsort", hash(qs, sizeof qs));
        show("interp", (u64)k_interp(code, 64, (i64)n - 500, n * 3));
        show("classify", (u64)k_classify_sum(cls, n));
        show("bits", k_bits(b64, n));
        show("narrow", (u64)k_narrow(nar, n));
        show("fconv", (u64)k_fconv(fc, n));
        show("collatz", k_collatz(1, n));
        memset(dst, 0, sizeof dst);
        show("copy", k_copy(dst, bytes, n));
        show("copy-dst", hash(dst, sizeof dst));
        show("dispatch", k_dispatch(a64, n));
        show("names", k_names(n));
        show("stackobj", (u64)k_stackobj(s64, n));
        snprintf(tag, sizeof tag, "matmul-%u", (unsigned)(n % 65));
        k_matmul(m1, m2, m3, (i32)(n % 65 > 64 ? 64 : n % 65));
        show(tag, hash(m3, sizeof m3));
    }
    printf("== scalar edges\n");
    i32 cv[] = {-1000, -7, 0, 3, 64, 1 << 20, 2147483647, -2147483647 - 1, 1, -1, 5, 6, 1000};
    for (unsigned i = 0; i < sizeof cv / sizeof *cv; ++i) show("classify1", (u64)(i64)k_classify(cv[i]));
    for (int v = -32768; v <= 32767; v += 257) {
        show("clamp", (u64)(i64)clamp_i8((i16)v));
        show("mix", (u64)mix_u16((u8)v, (i8)(v >> 3), (u16)(v * 7)));
    }
    u64 g[] = {0, 1, 2, 12, 18, 1ULL << 63, ~0ULL, 0xFFFFFFFFULL, 48, 180};
    for (unsigned i = 0; i < 10; ++i)
        for (unsigned j = 0; j < 10; ++j) show("gcd64", gcd64(g[i], g[j]));
    show("calls", kernel_calls);
    return 0;
}

static u64 now(void) { return clock_gettime_nsec_np(CLOCK_UPTIME_RAW); }
static volatile u64 sink;

static int timed(const char *name, u64 reps) {
    u64 c = 0, t0 = now();
    if (!strcmp(name, "logic")) for (u64 r = 0; r < reps; ++r) c += k_logic(a64, N);
    else if (!strcmp(name, "fnv1a")) for (u64 r = 0; r < reps; ++r) c += k_fnv1a(bytes, N);
    else if (!strcmp(name, "crc32")) for (u64 r = 0; r < reps; ++r) c += k_crc32(bytes, N);
    else if (!strcmp(name, "sieve")) for (u64 r = 0; r < reps; ++r) c += k_sieve(flags, sizeof flags);
    else if (!strcmp(name, "matmul")) for (u64 r = 0; r < reps; ++r) { k_matmul(m1, m2, m3, 64); c += (u64)m3[r & 4095]; }
    else if (!strcmp(name, "dot")) for (u64 r = 0; r < reps; ++r) c += dbits(k_dot(dx, dy, N));
    else if (!strcmp(name, "saxpy")) for (u64 r = 0; r < reps; ++r) { k_saxpy(1.0001f, fx, fy, N); c += (u64)fy[r & 4095]; }
    else if (!strcmp(name, "divmix")) for (u64 r = 0; r < reps; ++r) c += (u64)k_divmix(s64, d64, N);
    else if (!strcmp(name, "gcd")) for (u64 r = 0; r < reps; ++r) c += k_gcd(b64, N);
    else if (!strcmp(name, "isort")) for (u64 r = 0; r < reps; ++r) { memcpy(is, a64, 1024 * sizeof *is); k_isort(is, 1024); c += (u64)is[r & 1023]; }
    else if (!strcmp(name, "qsort")) for (u64 r = 0; r < reps; ++r) { memcpy(qs, a64, sizeof qs); k_qsort(qs, N); c += (u64)qs[r & 4095]; }
    else if (!strcmp(name, "interp")) for (u64 r = 0; r < reps; ++r) c += (u64)k_interp(code, 64, (i64)r, 100000);
    else if (!strcmp(name, "classify")) for (u64 r = 0; r < reps; ++r) c += (u64)k_classify_sum(cls, N);
    else if (!strcmp(name, "bits")) for (u64 r = 0; r < reps; ++r) c += k_bits(b64, N);
    else if (!strcmp(name, "narrow")) for (u64 r = 0; r < reps; ++r) c += (u64)k_narrow(nar, N);
    else if (!strcmp(name, "fconv")) for (u64 r = 0; r < reps; ++r) c += (u64)k_fconv(fc, N);
    else if (!strcmp(name, "collatz")) for (u64 r = 0; r < reps; ++r) c += k_collatz(1 + r, 2000);
    else if (!strcmp(name, "copy")) for (u64 r = 0; r < reps; ++r) c += k_copy(dst, bytes, N);
    else if (!strcmp(name, "dispatch")) for (u64 r = 0; r < reps; ++r) c += k_dispatch(a64, N);
    else if (!strcmp(name, "names")) for (u64 r = 0; r < reps; ++r) c += k_names(1000);
    else if (!strcmp(name, "stackobj")) for (u64 r = 0; r < reps; ++r) c += (u64)k_stackobj(s64, N);
    else { fprintf(stderr, "unknown kernel %s\n", name); return 2; }
    u64 t1 = now();
    sink = c;
    printf("%s %llu %016llx\n", name, (unsigned long long)(t1 - t0), (unsigned long long)c);
    return 0;
}

int main(int argc, char **argv) {
    init();
    if (argc == 2 && !strcmp(argv[1], "check")) return check();
    if (argc == 4 && !strcmp(argv[1], "time")) return timed(argv[2], strtoull(argv[3], 0, 10));
    fprintf(stderr, "usage: driver check | driver time NAME REPS\n");
    return 2;
}
