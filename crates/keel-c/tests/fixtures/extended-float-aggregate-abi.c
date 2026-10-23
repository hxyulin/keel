/* Negative Keel reduction at 8bb21e9a: this must eventually diagnose until
 * binary128 aggregate ABI transport is implemented. Native GCC/Clang accept.
 * See docs/c-system-header-compat.md for the independent cross-call harness. */
struct box { __float128 x; };
struct box pick(struct box a, struct box b) { return b; }
