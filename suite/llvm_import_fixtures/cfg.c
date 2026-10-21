#include <stdint.h>
uint64_t compiler_cfg(uint64_t x, volatile uint64_t *p) {
  if (x & 1) {
    p[2] = x;
    x ^= p[0];
  } else {
    p[1] = x + p[3];
    x += p[1];
  }
  return x ^ p[2];
}
