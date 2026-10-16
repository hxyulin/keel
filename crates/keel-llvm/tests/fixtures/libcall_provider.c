/* Compile separately with -fno-builtin; never include this body in exported IR. */
#include <stddef.h>
volatile unsigned hits;
size_t strlen(const char *s) { (void)s; ++hits; return 7; }
