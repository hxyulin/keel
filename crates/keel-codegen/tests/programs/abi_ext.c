#include <stdint.h>
#include <stdio.h>

/* Declared with the IR's narrow types in abi_ext.keel; here as int, to see
 * the whole 32-bit register. */
int32_t k_pass(int64_t);
int32_t k_pass_stack(int64_t);
int32_t k_ret_s8(int64_t);
int32_t k_ret_u8(int64_t);
int32_t k_ret_s16(int64_t);
int32_t k_ret_u16(int64_t);
int32_t k_use_ret(void);

static int failures;

#define CHECK(got, want)                                                      \
    do {                                                                      \
        long long got_ = (got), want_ = (want);                               \
        if (got_ != want_) {                                                  \
            printf("line %d: %s = %#llx, want %#llx\n", __LINE__, #got,       \
                   got_, want_);                                              \
            failures++;                                                       \
        }                                                                     \
    } while (0)

static int64_t seen[4];

/* Records the four registers exactly as they arrived. */
int32_t c_raw(int32_t a, int32_t b, int32_t c, int32_t d) {
    seen[0] = a, seen[1] = b, seen[2] = c, seen[3] = d;
    return 0;
}

int32_t c_stack(int64_t a0, int64_t a1, int64_t a2, int64_t a3, int64_t a4, int64_t a5,
                int64_t a6, int64_t a7, signed char s, short h, unsigned char u, int32_t w) {
    return s + h * 3 + u * 5 + w * 7;
}

signed char c_ret_s8(void) { return -100; }

int main(void) {
    /* Upper bits set everywhere, so a missing extension shows. */
    int64_t xs[] = {0x123456ff, 0x7f80, 0x12340001, -1, 0x5a5a5a5a5a5a8a81};
    for (int i = 0; i < 5; i++) {
        int64_t x = xs[i];
        k_pass(x);
        CHECK(seen[0], (int8_t)x);
        CHECK(seen[1], (int16_t)x);
        CHECK(seen[2], (uint8_t)x);
        CHECK(seen[3], (uint16_t)x);
        CHECK(k_pass_stack(x), (int8_t)x + (int16_t)x * 3 + (uint8_t)x * 5 + (int32_t)x * 7);
        CHECK(k_ret_s8(x), (int8_t)x);
        CHECK(k_ret_u8(x), (uint8_t)x);
        CHECK(k_ret_s16(x), (int16_t)x);
        CHECK(k_ret_u16(x), (uint16_t)x);
    }
    CHECK(k_use_ret(), -99);
    return failures;
}
