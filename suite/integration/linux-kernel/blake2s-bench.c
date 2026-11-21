// SPDX-License-Identifier: MIT
#define _POSIX_C_SOURCE 200809L
#include <errno.h>
#include <inttypes.h>
#include <limits.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

struct blake2s_state {
    uint32_t h[8], t[2], f[2];
    uint8_t buf[64];
    unsigned buflen, outlen;
};
struct vector {
    size_t size;
    unsigned pattern;
    const char *hex;
};

void blake2s_compress_generic(struct blake2s_state *, const uint8_t *, size_t, uint32_t);

// Python hashlib.blake2s digests for prepare() with each size and pattern.
static const struct vector vectors[] = {
    { 0, 0, "69217a3079908094e11121d042354a7c1f55b6482ca1a51e1b250dfd1ed0eef9" },
    { 1, 1, "4d343932a676c2a13d81ad1a0e0207f3e3329b0bf61d1b0fce8ded1e2b97463e" },
    { 3, 2, "66126f90a0a384c9ad846f9f6692ed8ade41dfd51ddc53d23667c119eadf9bc8" },
    { 63, 3, "6bad88342ffa70a2ae50dfcc4e9fb444eaf06ee6913c2aace92dd05ad94c1f19" },
    { 64, 4, "0cc97435f7c6d5c9f67a1c7a51947696eaf1c8e0b5c9ef24aca547c7a21c4464" },
    { 65, 5, "3790e333c51cc72d02675cb8e3eeaafd22e8f5cd0eb90df40d9390ccccb3ef2c" },
    { 127, 6, "6af881e9e1fdc8d9d282f5b94a81684327fb6fdae01e4ba1975e264ef9c50516" },
    { 128, 7, "95a0ae36de63915b59af42843943e0811b447af1b1a7cc0c4f402125358f2f68" },
    { 129, 8, "f305026f0c919c999ddd4ca50b8e9c6b4a014ee4c0167262bff9d17c03d4239c" },
    { 255, 9, "8df23dbe26fc37812eedbd693abdcb2df351c3dcc2f7ba48b01e70e96174529d" },
    { 512, 10, "7f06090d937c155908d5fc0d732ba51a1b4da145e90dc0af564ada3272656a39" },
    { 769, 11, "9159fa8d8f5828b40952db955cf2f9d1b5b684c5aa684590daaf6aef86d01f8f" },
    { 4096, 12, "f1d16dcb1458481437f148e88b8e1ec5fdb2dc50aa41a8f9def3071796943474" },
    { 65536, 13, "10e29e18091705e65f3c7f0b7383983657dbef9ac2a8ccf2a3140cecb8fb4c82" },
};

static const uint32_t iv[] = {
    0x6a09e667u ^ 0x01010020u, 0xbb67ae85u, 0x3c6ef372u, 0xa54ff53au,
    0x510e527fu, 0x9b05688cu, 0x1f83d9abu, 0x5be0cd19u
};

void *__wrap_memcpy(void *dst, const void *src, size_t n) {
    void *original = dst;
    __asm__ volatile("rep movsb" : "+D"(dst), "+S"(src), "+c"(n) : : "memory");
    return original;
}

static void prepare(const struct vector *vector, uint8_t *data) {
    for (size_t i = 0; i < vector->size; ++i)
        data[i] = (uint8_t)(i * 17 + (i >> 7) * 3 + vector->pattern * 77);
}

static struct blake2s_state hash_once(const uint8_t *data, size_t size) {
    struct blake2s_state state = {0};
    memcpy(state.h, iv, sizeof(iv));
    size_t whole = size ? (size - 1) / 64 : 0;
    blake2s_compress_generic(&state, data, whole, 64);
    size_t remainder = size - whole * 64;
    uint8_t tail[64] = {0};
    memcpy(tail, data + whole * 64, remainder);
    state.f[0] = UINT32_MAX;
    blake2s_compress_generic(&state, tail, 1, remainder);
    return state;
}

static int check(const struct blake2s_state *state, const struct vector *vector) {
    char hex[65];
    for (size_t i = 0; i < 32; ++i)
        snprintf(hex + 2 * i, 3, "%02x", (unsigned)(state->h[i / 4] >> (8 * (i % 4))) & 255u);
    if (strcmp(hex, vector->hex) || state->t[0] != vector->size || state->t[1]) {
        fprintf(stderr, "%zu bytes: got %s, expected %s\n", vector->size, hex, vector->hex);
        return 0;
    }
    return 1;
}

static uint64_t clock_ns(void) {
    struct timespec ts;
    if (clock_gettime(CLOCK_MONOTONIC_RAW, &ts) != 0)
        abort();
    return UINT64_C(1000000000) * ts.tv_sec + ts.tv_nsec;
}

int main(int argc, char **argv) {
    _Alignas(64) uint8_t buffer[65536 + 1];
    uint8_t *data = buffer + 1;
    for (size_t i = 0; i < sizeof(vectors) / sizeof(vectors[0]); i++) {
        const struct vector *vector = &vectors[i];
        prepare(vector, data);
        struct blake2s_state state = hash_once(data, vector->size);
        if (!check(&state, vector))
            return 1;
    }
    if (argc == 1)
        return 0;
    char *end;
    errno = 0;
    unsigned long repeat = strtoul(argv[1], &end, 10);
    if (argc != 2 || argv[1][0] < '0' || argv[1][0] > '9' ||
        *end || errno || !repeat || repeat > UINT_MAX) {
        fprintf(stderr, "usage: %s [positive iteration count]\n", argv[0]);
        return 2;
    }
    const struct vector *vector = &vectors[sizeof(vectors) / sizeof(vectors[0]) - 1];
    uint64_t start = clock_ns();
    uint32_t checksum = 0;
    for (unsigned i = 0; i < repeat; ++i) {
        struct blake2s_state state = hash_once(data, vector->size);
        checksum += state.h[i % 8];
    }
    uint64_t elapsed = clock_ns() - start;
    printf("%" PRIu64 " %08" PRIx32 "\n", elapsed, checksum);
    return 0;
}
