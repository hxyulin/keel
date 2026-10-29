/* Unsigned arithmetic avoids undefined signed shifts and out-of-range counts. */
static uint64_t shift_lane(int op, int bits, uint64_t x, uint64_t count) {
  uint64_t mask = UINT64_MAX >> (64-bits);
  x &= mask;
  if (op == 0) return count >= (unsigned)bits ? 0 : (x << count) & mask;
  if (op == 1) return count >= (unsigned)bits ? 0 : x >> count;
  int negative = (x >> (bits-1)) & 1;
  if (count >= (unsigned)bits) return negative ? mask : 0;
  uint64_t result = x >> count;
  if (negative && count) result |= mask ^ (mask >> count);
  return result;
}
