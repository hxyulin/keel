// C side of globals.keel: data and functions keel reaches through the GOT,
// and checks that C sees keel's exported data.
#include <stdint.h>
#include <string.h>

int64_t c_counter = 7;            // keel: decl data @c_counter
extern int64_t counter;           // keel's @counter
extern const unsigned char mixed[12];

int64_t c_read_counter(void) { return counter; }
int64_t c_read_c_counter(void) { return c_counter; }
int64_t c_double(int64_t x) { return 2 * x; }

typedef int64_t (*binop)(int64_t, int64_t);
int64_t c_apply(binop f, int64_t a, int64_t b) { return f(a, b); }
static int64_t c_mul(int64_t a, int64_t b) { return a * b; }
binop c_get_mul(void) { return c_mul; }

// keel's @mixed must be exactly these bytes, little-endian.
int32_t c_check_mixed(void) {
    static const unsigned char want[12] = {1, 0xff, 4, 3, 0xfe, 0xff, 0xff, 0xff, 0, 0, 0, 0};
    return memcmp(mixed, want, sizeof want) == 0;
}
