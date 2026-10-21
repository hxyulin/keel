#include <stdint.h>
#include <stdio.h>
extern uint64_t compiler_cfg(uint64_t, volatile uint64_t *);
int main(void) {
  uint64_t state = 0, hash = 0;
  for (unsigned i = 0; i < 8192; ++i) {
    state = state * UINT64_C(6364136223846793005) + 1;
    uint64_t p[4] = {state ^ 17, 2, state + 3, state ^ 99};
    uint64_t q[4] = {p[0], p[1], p[2], p[3]}, x = state;
    if (x & 1) { q[2] = x; x ^= q[0]; }
    else { q[1] = x + q[3]; x += q[1]; }
    x ^= q[2];
    if (compiler_cfg(state, p) != x) return 1;
    for (unsigned j = 0; j < 4; ++j) {
      if (p[j] != q[j]) return 2;
      hash = hash * 33 + p[j];
    }
    hash ^= x;
  }
  printf("%016llx\n", (unsigned long long)hash);
  return 0;
}
