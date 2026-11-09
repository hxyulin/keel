#include <stddef.h>
#include <stdint.h>
/* yyjson ASCII detection, interpreter tags, and 64-bit flag words. */
int ascii(unsigned c) { return (c & 128) == 0; }
int tag(unsigned c) { return (c & 15) != 0; }
int flags(uint64_t c) { return (c & UINT64_C(0x8000000000000001)) != 0; }
size_t count_ascii(const unsigned char *s, size_t n) {
    size_t count = 0;
    for (size_t i = 0; i < n; ++i) count += (s[i] & 128) == 0;
    return count;
}
