/* Public APIs only. Expected decoded bytes come from a deterministic generator
 * and a hand-built raw frame, rather than another call to the codec.
 * Timed modes use fixed work and keep contexts/buffers across iterations.
 */
#include "zstd.h"
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define BLOCK_SIZE 262144
#define CORPORA 3
static unsigned char input[CORPORA][BLOCK_SIZE];
static unsigned char decoded[BLOCK_SIZE + 16];
static unsigned char *encoded[CORPORA];
static size_t encoded_size[CORPORA];
static size_t capacity;
static unsigned checks;

static void require(int condition, const char *message) {
    ++checks;
    if (!condition) {
        fprintf(stderr, "zstd check %u failed: %s\n", checks, message);
        exit(1);
    }
}

static void success(size_t value) {
    if (ZSTD_isError(value)) {
        fprintf(stderr, "zstd error: %s\n", ZSTD_getErrorName(value));
        exit(1);
    }
}

static uint64_t hash(const unsigned char *p, size_t size) {
    uint64_t value = UINT64_C(14695981039346656037);
    size_t i;
    for (i = 0; i < size; ++i) {
        value ^= p[i];
        value *= UINT64_C(1099511628211);
    }
    return value;
}

static void prepare(void) {
    static const char text[] = "Zstandard public API workload: repeated records, changing values, and bytes.\n";
    uint32_t state = UINT32_C(0x9e3779b9);
    size_t i;
    int kind;
    capacity = ZSTD_compressBound(BLOCK_SIZE);
    for (kind = 0; kind < CORPORA; ++kind) {
        encoded[kind] = (unsigned char *)malloc(capacity);
        require(encoded[kind] != NULL, "allocate compressed buffer");
    }
    for (i = 0; i < BLOCK_SIZE; ++i) {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        input[0][i] = (unsigned char)text[i % (sizeof(text) - 1)];
        input[1][i] = (unsigned char)((i / 4096 + i % 17) & 255);
        input[2][i] = (unsigned char)(state >> 24);
    }
}

static void roundtrip(ZSTD_CCtx *cc, ZSTD_DCtx *dc, size_t size, int kind, int level) {
    size_t stored, got, i;
    stored = ZSTD_compressCCtx(cc, encoded[kind], capacity, input[kind], size, level);
    success(stored);
    require(stored <= capacity, "encoded capacity");
    require(ZSTD_getFrameContentSize(encoded[kind], stored) == size, "frame content size");
    memset(decoded, 0xa5, sizeof(decoded));
    got = ZSTD_decompressDCtx(dc, decoded, size, encoded[kind], stored);
    success(got);
    require(got == size, "decoded length");
    require(memcmp(input[kind], decoded, size) == 0, "decoded bytes");
    for (i = size; i < sizeof(decoded); ++i)
        if (decoded[i] != 0xa5) require(0, "output guard overwritten");
    require(ZSTD_isError(ZSTD_decompressDCtx(dc, decoded, sizeof(decoded), encoded[kind], stored - 1)),
            "truncated frame rejected");
    if (size != 0)
        require(ZSTD_isError(ZSTD_decompressDCtx(dc, decoded, size - 1, encoded[kind], stored)),
                "short output rejected");
}

static void correctness(ZSTD_CCtx *cc, ZSTD_DCtx *dc) {
    static const size_t sizes[] = {0, 1, 255, 256, 4096, 131071, 131072, 131073, BLOCK_SIZE};
    static const int levels[] = {1, 3, 9};
    /* Single-segment frame, five-byte content, one final raw block. */
    static const unsigned char raw[] = {0x28, 0xb5, 0x2f, 0xfd, 0x20, 5, 0x29, 0, 0, 'h', 'e', 'l', 'l', 'o'};
    size_t i, level, got;
    int kind;
    got = ZSTD_decompressDCtx(dc, decoded, sizeof(decoded), raw, sizeof(raw));
    success(got);
    require(got == 5 && memcmp(decoded, "hello", 5) == 0, "independent raw frame");
    for (kind = 0; kind < CORPORA; ++kind)
        for (level = 0; level < sizeof(levels) / sizeof(levels[0]); ++level)
            for (i = 0; i < sizeof(sizes) / sizeof(sizes[0]); ++i)
                roundtrip(cc, dc, sizes[i], kind, levels[level]);
    printf("zstd checks %u\n", checks);
}

static void benchmark(ZSTD_CCtx *cc, ZSTD_DCtx *dc, int decompress) {
    uint64_t digest = 0;
    int kind, iteration, repetitions = decompress ? 8192 : 4096;
    size_t got = 0;
    /* Prepare every frame before the decompression loop. */
    for (kind = 0; kind < CORPORA; ++kind) {
        encoded_size[kind] = ZSTD_compressCCtx(cc, encoded[kind], capacity, input[kind], BLOCK_SIZE, 3);
        success(encoded_size[kind]);
    }
    for (iteration = 0; iteration < repetitions; ++iteration) {
        kind = iteration % CORPORA;
        if (decompress) {
            got = ZSTD_decompressDCtx(dc, decoded, BLOCK_SIZE, encoded[kind], encoded_size[kind]);
            success(got);
            require(got == BLOCK_SIZE, "benchmark decoded length");
            /* Consume each result without making hashing the main workload. */
            digest += decoded[(unsigned)iteration * 509 % BLOCK_SIZE];
        } else {
            got = ZSTD_compressCCtx(cc, encoded[kind], capacity, input[kind], BLOCK_SIZE, 3);
            success(got);
            encoded_size[kind] = got;
        }
    }
    /* Validate the final output from each corpus outside the hot loop. Do not
     * print compressed lengths or representation: only decoded byte hashes. */
    for (kind = 0; kind < CORPORA; ++kind) {
        got = ZSTD_decompressDCtx(dc, decoded, BLOCK_SIZE, encoded[kind], encoded_size[kind]);
        success(got);
        require(got == BLOCK_SIZE && memcmp(decoded, input[kind], BLOCK_SIZE) == 0,
                "benchmark decoded bytes");
        digest ^= hash(decoded, got);
    }
    printf("zstd %s %016llx\n", decompress ? "decompress" : "compress", (unsigned long long)digest);
}

int main(int argc, char **argv) {
    ZSTD_CCtx *cc;
    ZSTD_DCtx *dc;
    int kind;
    if (argc != 2 || (strcmp(argv[1], "test") != 0 && strcmp(argv[1], "compress") != 0 && strcmp(argv[1], "decompress") != 0)) {
        fprintf(stderr, "usage: %s test|compress|decompress\n", argv[0]);
        return 2;
    }
    prepare();
    cc = ZSTD_createCCtx();
    dc = ZSTD_createDCtx();
    require(cc != NULL && dc != NULL, "allocate contexts");
    if (strcmp(argv[1], "test") == 0) correctness(cc, dc);
    else benchmark(cc, dc, strcmp(argv[1], "decompress") == 0);
    ZSTD_freeCCtx(cc);
    ZSTD_freeDCtx(dc);
    for (kind = 0; kind < CORPORA; ++kind) free(encoded[kind]);
    return 0;
}
