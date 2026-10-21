#include <stdint.h>
#include <stdio.h>
extern uint64_t cfg(uint64_t, uint64_t);
extern uint64_t memory(uint64_t, int32_t);
extern uint32_t bool_cfg(uint32_t);
extern uint64_t boundaries(uint8_t, const unsigned char *);
extern uint64_t irreducible(uint64_t, uint64_t);
extern uint64_t layout(const unsigned char *, int32_t);
extern uint64_t v1(uint64_t);
int main(void) {
  unsigned char bytes[128];
  for (unsigned i = 0; i < sizeof(bytes); ++i) bytes[i] = i * 37;
  for (int i = -2; i <= 2; ++i) {
    int a = 64 + 12 * i + 10, b = 64 + 9 * i + 7;
    uint64_t expected = bytes[a] + 256u * bytes[a+1] + bytes[b] + 256u * bytes[b+1];
    if (layout(bytes + 64, i) != expected) return 3;
  }
  for (unsigned i = 0; i < 256; ++i) {
    uint64_t k = i == 128 ? UINT64_C(0x8000000000000000) : i == 127 ? UINT64_C(0x7fffffffffffffff) : 0;
    if (boundaries(i, bytes) != (k ^ bytes[i == 127])) return 4;
  }
  uint64_t hash = 0, state = 7;
  for (unsigned j = 0; j < 4096; ++j) {
    state = state * UINT64_C(6364136223846793005) + 1;
    uint64_t n = j % 33, a = state + 17, b = state;
    if (v1(state) != state + 37) return 5;
    for (unsigned entry = 0; entry < 2; ++entry)
      if (irreducible(n, entry) != (((n & 1) ^ entry) ? 23 : 11)) return 6;
    for (uint64_t i = 0; i < n; ++i) {
      uint64_t next = a ^ i; a = b; b = next;
    }
    uint64_t expected = (a & 3) < 2 ? b : a;
    if (cfg(n, state) != expected || bool_cfg(j) != (j & 1)) return 1;
    for (int i = 0; i < 4; ++i) {
      uint64_t value = memory(state, i);
      if (value != (i == 0 ? 19 : state) + state) return 2;
      hash = hash * 33 + value;
    }
    hash ^= expected;
  }
  printf("%016llx\n", (unsigned long long)hash);
  return 0;
}
