#include <stdint.h>

int32_t k_printf(const char *, ...);

int main(void) { return k_printf("forwarded %d and %.2f and %s\n", 42, 2.5, "keel") != 31; }
