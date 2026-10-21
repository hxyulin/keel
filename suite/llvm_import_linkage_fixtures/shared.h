using U = unsigned long long;
// Runtime recursion survives -O0; both TUs emit the same ODR helper name.
inline U linkage_factor(unsigned n) { return n ? 2 * linkage_factor(n - 1) : 1; }
