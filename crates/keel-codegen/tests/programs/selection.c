#include <stdint.h>
#include "clobber.h"
void selection_clobber(void) { clobber(); }
extern uint32_t tree_iadd32(uint32_t, uint32_t);
extern uint32_t tree_isub32(uint32_t, uint32_t);
extern uint32_t tree_and32(uint32_t, uint32_t);
extern uint32_t tree_or32(uint32_t, uint32_t);
extern uint32_t tree_xor32(uint32_t, uint32_t);
extern uint32_t tree_muladd32(uint32_t, uint32_t, uint32_t);
extern uint32_t tree_mulsub32(uint32_t, uint32_t, uint32_t);
extern uint64_t tree_iadd64(uint64_t, uint64_t);
extern uint64_t tree_isub64(uint64_t, uint64_t);
extern uint64_t tree_and64(uint64_t, uint64_t);
extern uint64_t tree_or64(uint64_t, uint64_t);
extern uint64_t tree_xor64(uint64_t, uint64_t);
extern uint64_t tree_muladd64(uint64_t, uint64_t, uint64_t);
extern uint64_t tree_mulsub64(uint64_t, uint64_t, uint64_t);
extern uint64_t tree_across_call(uint64_t, uint64_t, uint64_t);
extern uint64_t tree_shared(uint64_t, uint64_t, uint64_t);
extern uint64_t tree_constant(uint64_t, uint64_t);
int main(void) {
 const uint64_t values[] = {0, 1, 2, 7, UINT32_MAX, UINT64_MAX, UINT64_C(0x8000000000000000), UINT64_C(0x123456789abcdef0)};
 for (unsigned i=0; i<8; ++i) for (unsigned j=0; j<8; ++j) {
 uint64_t a=values[i], b=values[j], d=values[(i+j)%8];
if (tree_iadd32(a, b) != (uint32_t)((uint32_t)a + ((uint32_t)b << 2))) return 1;
if (tree_isub32(a, b) != (uint32_t)((uint32_t)a - ((uint32_t)b << 2))) return 2;
if (tree_and32(a, b) != (uint32_t)((uint32_t)a & ((uint32_t)b << 2))) return 3;
if (tree_or32(a, b) != (uint32_t)((uint32_t)a | ((uint32_t)b << 2))) return 4;
if (tree_xor32(a, b) != (uint32_t)((uint32_t)a ^ ((uint32_t)b << 2))) return 5;
if (tree_muladd32(a, b, d) != (uint32_t)(a * b + d)) return 6;
if (tree_mulsub32(a, b, d) != (uint32_t)(d - a * b)) return 7;
if (tree_iadd64(a, b) != (uint64_t)((uint64_t)a + ((uint64_t)b << 2))) return 8;
if (tree_isub64(a, b) != (uint64_t)((uint64_t)a - ((uint64_t)b << 2))) return 9;
if (tree_and64(a, b) != (uint64_t)((uint64_t)a & ((uint64_t)b << 2))) return 10;
if (tree_or64(a, b) != (uint64_t)((uint64_t)a | ((uint64_t)b << 2))) return 11;
if (tree_xor64(a, b) != (uint64_t)((uint64_t)a ^ ((uint64_t)b << 2))) return 12;
if (tree_muladd64(a, b, d) != (uint64_t)(a * b + d)) return 13;
if (tree_mulsub64(a, b, d) != (uint64_t)(d - a * b)) return 14;
if (tree_across_call(a, b, d) != a*b+d) return 20;
if (tree_shared(a,b,d) != ((a*b+d) ^ (a*b))) return 21;
if (tree_constant(a,b) != a*7+b) return 22;
} return 0; }
