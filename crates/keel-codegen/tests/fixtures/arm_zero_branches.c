/* Header-free independent C truth oracle. No signed arithmetic overflow. */
typedef unsigned char u8;
typedef unsigned short u16;
typedef unsigned int u32;
typedef unsigned long long u64;
#define DECL(B, T) \
    extern u64 f##B##_0_0(T); extern u64 f##B##_1_0(T); \
    extern u64 f##B##_0_1(T); extern u64 f##B##_1_1(T);
DECL(8, u8) DECL(16, u16) DECL(32, u32) DECL(64, u64)
#define CHECK(B, T, X) do { \
    T v = (T)(X); \
    if (f##B##_0_0(v) != (v ? 37 : 91)) return B; \
    if (f##B##_1_0(v) != (v ? 91 : 37)) return B + 1; \
    if (f##B##_0_1(v) != (v ? 37 : 91)) return B + 2; \
    if (f##B##_1_1(v) != (v ? 37 : 91)) return B + 3; \
} while (0)
int main(void) {
    const u64 values[] = {0, 1, 2, 128, 256, 32768, 65536,
        0x80000000ULL, 0x100000000ULL, 0x8000000000000000ULL,
        0xffffffffULL, 0xffffffffffffffffULL, 0xffffffff00000000ULL};
    for (unsigned i = 0; i < sizeof(values) / sizeof(values[0]); ++i) {
        CHECK(8, u8, values[i]); CHECK(16, u16, values[i]);
        CHECK(32, u32, values[i]); CHECK(64, u64, values[i]);
    }
    /* All byte values, including nonboolean and sign-bit cases. */
    for (unsigned i = 0; i < 256; ++i) CHECK(8, u8, i);
    return 0;
}
