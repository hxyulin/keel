/* Scalar C kernels for the Keel -> Cranelift bridge comparison. One translation
 * unit, compiled by `keel cc` to Keel IR; native Keel and Cranelift then
 * generate code from the same IR. The driver (driver.c) is compiled
 * separately by Clang. Only ordinary scalar/pointer signatures are used:
 * no aggregates by value, no varargs, no VLAs, no TLS. Integer overflow is
 * kept to unsigned types; signed division never divides by zero or
 * MIN/-1, so C semantics are defined everywhere. */
typedef unsigned long long u64;
typedef long long i64;
typedef unsigned int u32;
typedef int i32;
typedef unsigned short u16;
typedef short i16;
typedef unsigned char u8;
typedef signed char i8;

void *memcpy(void *, const void *, unsigned long);
void *memset(void *, int, unsigned long);

/* Mutable global and constant table with relocations to data. */
u64 kernel_calls;
static const u32 crc_poly[2] = {0u, 0xEDB88320u};
static const char *const names[3] = {"alpha", "beta", "gamma"};

/* Logical-immediate array throughput (suite/arm-codegen case 8 shape). */
u64 k_logic(const u64 *p, u64 n) {
    u64 sum = 0, m = 0x00ff00ff00ff00ffULL;
    for (u64 i = 0; i < n; ++i) {
        u64 x = p[i], y = p[n + i];
        sum += (x & m) + (y ^ m) + (x | m);
    }
    kernel_calls++;
    return sum;
}

/* FNV-1a over bytes: byte loads, 32-bit multiply. */
u32 k_fnv1a(const u8 *s, u64 n) {
    u32 h = 2166136261u;
    for (u64 i = 0; i < n; ++i) {
        h ^= s[i];
        h *= 16777619u;
    }
    return h;
}

/* Bitwise CRC-32: shifts, table select via data relocation. */
u32 k_crc32(const u8 *s, u64 n) {
    u32 c = 0xFFFFFFFFu;
    for (u64 i = 0; i < n; ++i) {
        c ^= s[i];
        for (int k = 0; k < 8; ++k)
            c = (c >> 1) ^ crc_poly[c & 1];
    }
    return ~c;
}

/* Sieve of Eratosthenes on a caller buffer: byte stores, nested loops. */
u64 k_sieve(u8 *flags, u64 n) {
    memset(flags, 1, n);
    u64 count = 0;
    for (u64 i = 2; i < n; ++i) {
        if (!flags[i]) continue;
        ++count;
        for (u64 j = i * i; j < n; j += i) flags[j] = 0;
    }
    return count;
}

/* Signed 32-bit matrix multiply, c = a * b, n x n row-major. */
void k_matmul(const i32 *a, const i32 *b, i32 *c, i32 n) {
    for (i32 i = 0; i < n; ++i)
        for (i32 j = 0; j < n; ++j) {
            i32 s = 0;
            for (i32 k = 0; k < n; ++k) s += a[i * n + k] * b[k * n + j];
            c[i * n + j] = s;
        }
}

/* Double dot product and float saxpy. */
double k_dot(const double *x, const double *y, u64 n) {
    double s = 0.0;
    for (u64 i = 0; i < n; ++i) s += x[i] * y[i];
    return s;
}

void k_saxpy(float a, const float *x, float *y, u64 n) {
    for (u64 i = 0; i < n; ++i) y[i] = a * x[i] + y[i];
}

/* Signed and unsigned division/remainder with mixed signs. */
i64 k_divmix(const i64 *a, const i64 *b, u64 n) {
    i64 s = 0;
    for (u64 i = 0; i < n; ++i) {
        i64 x = a[i], y = b[i];
        s += x / y + x % y;
        s ^= (i64)((u64)x / (u64)(y | 1)) + (i64)((u64)x % 7u);
        s += (i32)x / 3 - (i32)x % -5;
    }
    return s;
}

/* Euclid's GCD summed over pairs: urem loop. */
u64 gcd64(u64 a, u64 b) {
    while (b) {
        u64 t = a % b;
        a = b;
        b = t;
    }
    return a;
}

u64 k_gcd(const u64 *a, u64 n) {
    u64 s = 0;
    for (u64 i = 0; i + 1 < n; ++i) s += gcd64(a[i], a[i + 1]);
    return s;
}

/* Insertion sort on signed ints: signed compares, memory moves. */
void k_isort(i32 *a, u64 n) {
    for (u64 i = 1; i < n; ++i) {
        i32 v = a[i];
        u64 j = i;
        while (j > 0 && a[j - 1] > v) {
            a[j] = a[j - 1];
            --j;
        }
        a[j] = v;
    }
}

/* Recursive quicksort: direct recursive calls. */
static void qsort_rec(i64 *a, i64 lo, i64 hi) {
    while (lo < hi) {
        i64 p = a[(lo + hi) / 2], i = lo, j = hi;
        while (i <= j) {
            while (a[i] < p) ++i;
            while (a[j] > p) --j;
            if (i <= j) {
                i64 t = a[i];
                a[i] = a[j];
                a[j] = t;
                ++i;
                --j;
            }
        }
        if (j - lo < hi - i) {
            qsort_rec(a, lo, j);
            lo = i;
        } else {
            qsort_rec(a, i, hi);
            hi = j;
        }
    }
}

void k_qsort(i64 *a, u64 n) {
    if (n) qsort_rec(a, 0, (i64)n - 1);
}

/* A small bytecode interpreter: dense switch, loop-carried state. */
i64 k_interp(const u8 *code, u64 len, i64 x, u64 steps) {
    i64 acc = x, r = 1;
    u64 pc = 0;
    while (steps--) {
        switch (code[pc]) {
        case 0: acc += r; break;
        case 1: acc -= 3; break;
        case 2: acc *= 3; break;
        case 3: acc ^= acc >> 7; break;
        case 4: r = acc & 15; break;
        case 5: acc = acc < 0 ? -acc : acc; break;
        case 6: acc = (acc << 3) - acc; break;
        case 7: if (acc & 1) pc = (pc + 2) % len; break;
        default: acc = ~acc; break;
        }
        pc = (pc + 1) % len;
    }
    return acc + r;
}

/* Sparse switch on a signed value. */
i32 k_classify(i32 v) {
    switch (v) {
    case -1000: return 1;
    case -7: return 2;
    case 0: return 3;
    case 3: return 4;
    case 64: return 5;
    case 1 << 20: return 6;
    case 2147483647: return 7;
    default: return v & 1 ? -1 : -2;
    }
}

i64 k_classify_sum(const i32 *v, u64 n) {
    i64 s = 0;
    for (u64 i = 0; i < n; ++i) s += k_classify(v[i]) * (i64)(i & 7);
    return s;
}

/* Bit manipulation through builtins and rotate idioms. */
u64 k_bits(const u64 *p, u64 n) {
    u64 s = 0;
    for (u64 i = 0; i < n; ++i) {
        u64 x = p[i];
        u32 r = (u32)(x & 63);
        u64 rot = (x << r) | (x >> ((64 - r) & 63));
        s += (u64)__builtin_popcountll(x) + (x ? (u64)__builtin_clzll(x) : 64);
        s += x ? (u64)__builtin_ctzll(x) : 64;
        s ^= rot + __builtin_bswap64(x) + __builtin_bswap32((u32)x);
    }
    return s;
}

/* Narrow integer arithmetic and narrow ABI parameters/returns. */
i8 clamp_i8(i16 v) {
    return (i8)(v < -128 ? -128 : v > 127 ? 127 : v);
}

u16 mix_u16(u8 a, i8 b, u16 c) {
    return (u16)(a * 3 + b * 5 + (c >> 2));
}

i64 k_narrow(const i16 *p, u64 n) {
    i64 s = 0;
    for (u64 i = 0; i < n; ++i) {
        i16 v = p[i];
        i8 c = clamp_i8(v);
        u16 m = mix_u16((u8)v, (i8)(v >> 8), (u16)v);
        s += c * 7 + m - (i16)(v * 3) / 7 + (u8)(v >> 3);
    }
    return s;
}

/* Floating conversions and NaN-aware comparisons. */
i64 k_fconv(const double *d, u64 n) {
    i64 s = 0;
    for (u64 i = 0; i < n; ++i) {
        double x = d[i];
        if (x != x) { s += 1000; continue; }
        if (x > -1e9 && x < 1e9) s += (i64)x + (i64)(i32)(x * 0.5);
        float f = (float)x;
        s += (f < 0.0f) - (f > 0.0f) + (x == 0.0) * 3;
        s += (i64)(u32)(x > 0 && x < 4e9 ? (u32)x : 0u);
    }
    return s;
}

/* Collatz step counts: branches, shifts, multiply. */
u64 k_collatz(u64 start, u64 n) {
    u64 total = 0;
    for (u64 i = start; i < start + n; ++i) {
        u64 x = i;
        while (x != 1) {
            x = (x & 1) ? 3 * x + 1 : x >> 1;
            ++total;
        }
    }
    return total;
}

/* memcpy/memset through calls, with variable lengths. */
u64 k_copy(u8 *dst, const u8 *src, u64 n) {
    u64 s = 0;
    for (u64 len = 1; len <= n; len = len * 2 + 1) {
        memcpy(dst, src, len);
        memset(dst + len / 2, (int)(len & 0xff), len / 4);
        for (u64 i = 0; i < len; ++i) s = s * 31 + dst[i];
    }
    return s;
}

/* Indirect calls through a table of function pointers. */
static u64 op_add(u64 a, u64 b) { return a + b; }
static u64 op_xor(u64 a, u64 b) { return a ^ b; }
static u64 op_mul(u64 a, u64 b) { return a * (b | 1); }
static u64 op_rot(u64 a, u64 b) { return (a << 13 | a >> 51) + b; }
static u64 (*const ops[4])(u64, u64) = {op_add, op_xor, op_mul, op_rot};

u64 k_dispatch(const u64 *p, u64 n) {
    u64 acc = 0x12345;
    for (u64 i = 0; i < n; ++i) acc = ops[p[i] & 3](acc, p[i]);
    return acc;
}

/* String table through constant pointer data. */
u64 k_names(u64 n) {
    u64 s = 0;
    for (u64 i = 0; i < n; ++i) {
        const char *p = names[i % 3];
        while (*p) s = s * 33 + (u8)*p++;
    }
    return s;
}

/* Small struct on the stack via address (no by-value aggregates). */
/* Accumulators and the result wrap modulo 2^64, including extreme inputs. */
struct acc { u64 lo, hi; u32 count; };
static void acc_add(struct acc *a, i64 v) {
    if (v < 0) a->lo += v; else a->hi += v;
    a->count++;
}

u64 k_stackobj(const i64 *v, u64 n) {
    struct acc a;
    memset(&a, 0, sizeof a);
    for (u64 i = 0; i < n; ++i) acc_add(&a, v[i]);
    return a.lo * 3 + a.hi + a.count;
}
