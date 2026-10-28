/* Deterministic block workload; only stable public lz4.h APIs are used.
 * Checksums cover decoded/source bytes, never compressed representation.
 * Bench modes use fixed work, with allocation/corpus preparation outside loops.
 */
#include "lz4.h"
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define MAX_SIZE 65537
#define CORPORA 3
#define BLOCK_SIZE 65536
#define ITERATIONS 12000

static unsigned char input[MAX_SIZE];
static unsigned char decoded[MAX_SIZE + 16];
static char compressed[LZ4_COMPRESSBOUND(MAX_SIZE)];
static uint64_t digest = UINT64_C(14695981039346656037);
static unsigned checks;

static void require(int condition, const char *message) {
    ++checks;
    if (!condition) {
        fprintf(stderr, "lz4 check %u failed: %s\n", checks, message);
        exit(1);
    }
}

static void checksum(const unsigned char *bytes, int size) {
    int i;
    for (i = 0; i < size; ++i) {
        digest ^= bytes[i];
        digest *= UINT64_C(1099511628211);
    }
    digest ^= (uint32_t)size;
    digest *= UINT64_C(1099511628211);
}

static void corpus(unsigned char *out, int size, int kind) {
    static const char text[] =
        "LZ4 deterministic compression workload: the quick brown fox jumps "
        "over the lazy dog. Public APIs preserve every decoded byte.\n";
    uint32_t rng = UINT32_C(0x9e3779b9);
    int i;
    for (i = 0; i < size; ++i) {
        rng ^= rng << 13;
        rng ^= rng >> 17;
        rng ^= rng << 5;
        if (kind == 0) out[i] = (unsigned char)text[(unsigned)i % (sizeof(text) - 1)];
        else if (kind == 1) out[i] = (unsigned char)((i / 4096) & 3);
        else out[i] = (unsigned char)(rng >> 24);
    }
}

static int encode(const unsigned char *src, char *dst, int size, int cap, int variant) {
    if (variant == 0) return LZ4_compress_default((const char *)src, dst, size, cap);
    return LZ4_compress_fast((const char *)src, dst, size, cap, 4);
}

static void roundtrip(int size, int kind, int variant) {
    int bound = LZ4_compressBound(size);
    int encoded, got, cut, probe, i;
    corpus(input, size, kind);
    require(bound > size && bound <= (int)sizeof(compressed), "compression bound");
    encoded = encode(input, compressed, size, bound, variant);
    require(encoded > 0 && encoded <= bound, "compress success");
    memset(decoded, 0xa5, sizeof(decoded));
    got = LZ4_decompress_safe(compressed, (char *)decoded, encoded, size);
    require(got == size, "decoded length");
    require(memcmp(input, decoded, (size_t)size) == 0, "decoded bytes");
    for (i = size; i < (int)sizeof(decoded); ++i)
        if (decoded[i] != 0xa5) require(0, "output guard overwritten");
    checksum(decoded, got);
    /* Zero capacity must fail compression, including for the empty block. */
    require(encode(input, compressed, size, 0, variant) == 0, "zero compression capacity");
    /* A failed compression call may alter its destination: encode again. */
    encoded = encode(input, compressed, size, bound, variant);
    require(encoded > 0, "recompress");
    if (size > 0) {
        require(LZ4_decompress_safe(compressed, (char *)decoded, encoded, size - 1) < 0,
                "short decode capacity");
        /* A truncated prefix can itself be a valid shorter block. It must
         * never reproduce the complete original payload. */
        /* Fixed probe count keeps stdout independent of encoded length. */
        for (probe = 0; probe < 3; ++probe) {
            cut = probe == 0 ? 0 : probe == 1 ? 1 : encoded / 2;
            got = LZ4_decompress_safe(compressed, (char *)decoded, cut, size);
            require(got < size, "truncated prefix cannot decode full payload");
        }
        got = LZ4_decompress_safe(compressed, (char *)decoded, encoded - 1, size);
        require(got < size, "last byte truncation");
    }
}

static void correctness(void) {
    /* Literal/match thresholds, machine word edges, and the 64 KiB window. */
    static const int sizes[] = {
        0, 1, 2, 3, 4, 5, 7, 8, 9, 12, 13, 14, 15, 16, 17,
        31, 32, 33, 254, 255, 256, 257, 4095, 4096, 4097,
        65534, 65535, 65536, 65537
    };
    static const unsigned char invalid[][5] = {
        {0xf0, 0xff, 0xff, 0xff, 0xff}, /* unterminated literal length */
        {0x10, 0x61, 0x01, 0x00, 0xff}, /* truncated final literals */
        {0x00, 0xff, 0xff, 0x00, 0x00}  /* match before output start */
    };
    unsigned s;
    int kind, variant;
    require(LZ4_versionNumber() == 11000, "pinned version number");
    require(strcmp(LZ4_versionString(), "1.10.0") == 0, "pinned version string");
    require(LZ4_compressBound(-1) == 0, "invalid source size bound");
    for (kind = 0; kind < CORPORA; ++kind)
        for (s = 0; s < sizeof(sizes) / sizeof(sizes[0]); ++s)
            for (variant = 0; variant < 2; ++variant)
                roundtrip(sizes[s], kind, variant);
    for (s = 0; s < sizeof(invalid) / sizeof(invalid[0]); ++s)
        require(LZ4_decompress_safe((const char *)invalid[s], (char *)decoded,
                                   5, MAX_SIZE) < 0, "malformed block");
    require(LZ4_decompress_safe("", (char *)decoded, 0, MAX_SIZE) < 0, "empty encoded input");
    /* The empty input's valid encoding is one zero token. */
    require(LZ4_decompress_safe("\0", (char *)decoded, 1, 0) == 0, "empty block");
    printf("lz4-1.10.0 test cases=174 checks=%u decoded-fnv64=%016llx\n",
           checks, (unsigned long long)digest);
}

static void benchmark(int decompress) {
    unsigned char *src = (unsigned char *)malloc(CORPORA * BLOCK_SIZE);
    int bound = LZ4_compressBound(BLOCK_SIZE);
    char *blocks = (char *)malloc((size_t)CORPORA * bound);
    int lengths[CORPORA];
    int i, k, got;
    uint64_t total = 0;
    require(src != NULL && blocks != NULL, "benchmark allocation");
    for (k = 0; k < CORPORA; ++k) {
        corpus(src + k * BLOCK_SIZE, BLOCK_SIZE, k);
        lengths[k] = LZ4_compress_default((const char *)src + k * BLOCK_SIZE,
                                         blocks + k * bound, BLOCK_SIZE, bound);
        require(lengths[k] > 0, "benchmark preparation");
    }
    for (i = 0; i < ITERATIONS; ++i) {
        k = i % CORPORA;
        if (decompress) {
            got = LZ4_decompress_safe(blocks + k * bound, (char *)decoded,
                                      lengths[k], BLOCK_SIZE);
            require(got == BLOCK_SIZE, "benchmark decoded size");
            total += decoded[(unsigned)i % BLOCK_SIZE];
        } else {
            got = LZ4_compress_default((const char *)src + k * BLOCK_SIZE,
                                       blocks + k * bound, BLOCK_SIZE, bound);
            require(got > 0, "benchmark compress");
            lengths[k] = got;
        }
    }
    for (k = 0; k < CORPORA; ++k) {
        got = LZ4_decompress_safe(blocks + k * bound, (char *)decoded,
                                  lengths[k], BLOCK_SIZE);
        require(got == BLOCK_SIZE, "benchmark final decoded size");
        require(memcmp(decoded, src + k * BLOCK_SIZE, BLOCK_SIZE) == 0,
                "benchmark final decoded bytes");
        checksum(decoded, got);
    }
    printf("lz4-1.10.0 %s iterations=%d bytes=%llu sample=%llu decoded-fnv64=%016llx\n",
           decompress ? "decompress" : "compress", ITERATIONS,
           (unsigned long long)ITERATIONS * BLOCK_SIZE,
           (unsigned long long)total, (unsigned long long)digest);
    free(blocks);
    free(src);
}

int main(int argc, char **argv) {
    if (argc == 2 && strcmp(argv[1], "test") == 0) correctness();
    else if (argc == 2 && strcmp(argv[1], "compress") == 0) benchmark(0);
    else if (argc == 2 && strcmp(argv[1], "decompress") == 0) benchmark(1);
    else {
        fprintf(stderr, "usage: %s test|compress|decompress\n", argv[0]);
        return 2;
    }
    return 0;
}
