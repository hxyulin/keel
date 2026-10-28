#include "shared.h"
extern "C" U linkage_left(U x) { return x * linkage_factor(5); }
// Unused definitions must survive import too.
extern "C" U linkage_unused(U x) { return x ^ 91; }
