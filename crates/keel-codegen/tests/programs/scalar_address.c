#include <stdint.h>
#include <string.h>
extern uint32_t scalar_bool_value(uint32_t);
extern int64_t scalar_bool(int64_t, int64_t);
extern int64_t scalar_read8(void *);
extern uint16_t scalar_write16(void *, uint16_t);
extern int64_t scalar_call(void *);
extern int64_t scalar_pressure(void *, void *, void *, void *, void *, void *);
void scalar_touch(void *p) { ((uint8_t *)p)[8] ^= 0xa5; }
int main(void) {
  uint64_t rng = 0x5a81730db029ef61ULL;
  _Alignas(8) uint8_t data[6][32];
  memset(data, 0, sizeof(data));
  for (unsigned i = 0; i < 4096; ++i) {
    rng ^= rng << 13; rng ^= rng >> 7; rng ^= rng << 17;
    if (scalar_bool_value((uint32_t)rng) != ((rng & 15) != 0)) return 9;
    int64_t a = (int64_t)rng, b = (int64_t)(rng << 1);
    if (scalar_bool(a, b) != (a < b ? b : a)) return 6;
    if (scalar_bool(INT64_MIN, INT64_MAX) != INT64_MAX) return 7;
    if (scalar_bool(a, a) != a) return 8;
    unsigned sum = 0;
    for (unsigned j = 0; j < 6; ++j) { data[j][8] = (uint8_t)(rng >> (8*j)); sum += data[j][8]; }
    if (scalar_read8(data[0]) != (int8_t)data[0][8]) return 1;
    if (scalar_pressure(data[0], data[1], data[2], data[3], data[4], data[5]) != sum) return 2;
    uint16_t value = (uint16_t)rng, actual;
    if (scalar_write16(data[0], value) != value) return 3;
    memcpy(&actual, data[0] + 14, sizeof(actual));
    if (actual != value) return 4;
    int8_t expected = (int8_t)(data[0][8] ^ 0xa5);
    if (scalar_call(data[0]) != expected) return 5;
  }
  return 0;
}
