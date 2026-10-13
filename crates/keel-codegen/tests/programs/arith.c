#include <stdint.h>
#include <stdio.h>

int32_t udiv32(int32_t, int32_t);
int64_t sdiv64(int64_t, int64_t);
int32_t srem32(int32_t, int32_t);
int64_t urem64(int64_t, int64_t);
int32_t sdiv8(int32_t, int32_t);
int32_t add8(int32_t, int32_t);
int32_t mul32(int32_t, int32_t);
int32_t shl8(int32_t, int32_t);
int32_t ushr8(int32_t, int32_t);
int32_t sshr16(int32_t, int32_t);
int64_t shl64(int64_t, int64_t);
int64_t bitops(int64_t, int64_t);
int32_t slt8(int32_t, int32_t);
int32_t ult16(int32_t, int32_t);
int32_t ugt64(int64_t, int64_t);
int64_t sext_low_byte(int64_t);
int64_t uext32(int64_t);
int64_t big_const(void);
int64_t neg_const(void);
void store16(void *, int32_t);
int32_t load_u8(const void *);
int64_t swap_loop(int64_t, int64_t, int64_t);
int64_t max64(int64_t, int64_t);

static int failures;

#define CHECK(expr, want)                                                     \
    do {                                                                      \
        long long got_ = (long long)(expr), want_ = (long long)(want);        \
        if (got_ != want_) {                                                  \
            printf("%s = %lld, want %lld\n", #expr, got_, want_);             \
            failures++;                                                       \
        }                                                                     \
    } while (0)

int main(void) {
    CHECK(udiv32(-1, 2), (int32_t)(0xffffffffu / 2));
    CHECK(udiv32(7, 3), 2);
    CHECK(sdiv64(-7, 2), -3);
    CHECK(sdiv64(INT64_MIN, 2), INT64_MIN / 2);
    CHECK(srem32(-7, 2), -1);
    CHECK(srem32(INT32_MIN, -1), 0);
    CHECK(urem64(-1, 10), (int64_t)(UINT64_MAX % 10));
    CHECK(sdiv8(-128, 2), -64);
    CHECK(sdiv8(0x1ff, 3), 0); /* low byte 0xff = -1 */
    CHECK(add8(100, 100), -56);
    CHECK(mul32(65536, 65536), 0);
    CHECK(mul32(-3, 7), -21);
    CHECK(shl8(1, 7), 128);
    CHECK(shl8(1, 9), 2); /* amount taken mod 8 */
    CHECK(ushr8(-1, 4), 0x0f);
    CHECK(ushr8(0x80, 15), 1); /* 15 mod 8 = 7 */
    CHECK(sshr16(-32768, 15), -1);
    CHECK(sshr16(0x10000 | 0x4000, 14), 1); /* bits above 16 ignored */
    CHECK(shl64(1, 63), INT64_MIN);
    CHECK(shl64(1, 64), 1); /* amount taken mod 64 */
    CHECK(bitops(12, 10), ((12 & 10) ^ (12 | 10)) - 10);
    CHECK(slt8(-1, 1), 1);
    CHECK(slt8(0xff, 0x01), 1); /* 0xff is -1 as i8 */
    CHECK(slt8(1, -1), 0);
    CHECK(ult16(0xffff, 1), 0);
    CHECK(ult16(1, 0x1ffff), 1); /* 0xffff unsigned */
    CHECK(ugt64(-1, 1), 1);
    CHECK(ugt64(1, -1), 0);
    CHECK(sext_low_byte(0x1234ff), -1);
    CHECK(sext_low_byte(0x7f), 127);
    CHECK(uext32(-1), 0xffffffffLL);
    CHECK(big_const(), 0x123456789abcdef0LL);
    CHECK(neg_const(), -2);

    uint16_t halves[3] = {0x1111, 0x2222, 0x3333};
    store16(&halves[1], 0x7abcd);
    CHECK(halves[0], 0x1111);
    CHECK(halves[1], 0xabcd);
    CHECK(halves[2], 0x3333);

    uint8_t byte = 0xfe;
    CHECK(load_u8(&byte), 254);

    CHECK(swap_loop(1, 2, 0), 12);
    CHECK(swap_loop(1, 2, 1), 21);
    CHECK(swap_loop(1, 2, 3), 21);
    CHECK(swap_loop(1, 2, 4), 12);
    CHECK(max64(-5, 3), 3);
    CHECK(max64(9, 3), 9);
    /* An exit status is taken mod 256. */
    return failures != 0;
}
