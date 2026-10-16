/* One independently compiled translation unit per CASE, shared by all compilers.
 * Unsigned arithmetic is modulo 2^64; no volatile or compiler-specific hints. */
typedef unsigned long long u64;
typedef unsigned int u32;
u64 kernel(const u64 *p, unsigned long n) {
#if CASE == 0
    return n; /* External-call floor, deliberately no payload. */
#else
#if CASE == 12
    u64 a=0,b=0,c=0,d=0,e=0,f=0,g=0,h=0;
#else
    u64 sum = 0;
#endif
    for (unsigned long i = 0; i < n; ++i) {
        u64 x = p[i], y = p[n + i], z;
#if CASE == 1
        z = (u32)x & 0x00ff00ffU;
#elif CASE == 2
        z = (u32)x | 0x80000001U;
#elif CASE == 3
        z = (u32)x ^ 0xff00ff00U;
#elif CASE == 4
        z = x & 0x00ff00ff00ff00ffULL;
#elif CASE == 5
        z = x | 0x8000000000000001ULL;
#elif CASE == 6
        z = x ^ 0xff00ff00ff00ff00ULL;
#elif CASE == 7
        z = x & 0x0123456789abcdefULL; /* Nonencodable mask. */
#elif CASE == 8
        u64 m = 0x00ff00ff00ff00ffULL;
        z = (x & m) + (y ^ m) + (x | m); /* Shared constant, three consumers. */
#elif CASE == 9
        z = ((u32)x & 0x80000001U) + (y ^ 0x8000000000000001ULL);
#elif CASE == 10
        z = x + 0x0000000012340000ULL; /* Sparse MOVZ seed. */
#elif CASE == 11
        z = x + 0xffff1234ffffffffULL; /* Negative bit pattern, MOVN seed. */
#elif CASE == 12
        /* Eight live independent accumulators expose allocator interactions. */
        z = (x & 0x00ff00ff00ff00ffULL) + (y ^ 0x8000000000000001ULL);
#else
#error unknown CASE
#endif
#if CASE == 12
        a += z; b += x ^ 0xff00ff00ff00ff00ULL;
        c += y | 0x8000000000000001ULL; d += x + 0xffff1234ffffffffULL;
        e += y & 0x0123456789abcdefULL; f += (u32)x | 0x80000001U;
        g += (u32)y ^ 0xff00ff00U; h += y + 0x12340000ULL;
#else
        sum += z;
#endif
    }
#if CASE == 12
    return a + b + c + d + e + f + g + h;
#else
    return sum;
#endif
#endif
}
