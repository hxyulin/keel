/* Compile-only width/AVX ABI probe; not part of the timed pointer-ABI corpus. */
typedef unsigned V __attribute__((vector_size(32)));
V kernel(V a, V b) { return a + b; }
