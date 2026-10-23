/* Capability probe: Keel C deliberately exposes only vector_size(16). */
typedef unsigned V __attribute__((vector_size(32)));
void kernel(V *d, const V *a, const V *b) { *d = *a + *b; }
