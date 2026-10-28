#include <stdint.h>
#include <stdio.h>
extern long leaf(long);
extern long padding_leaf(long);
extern long call_only(long);
extern long choose(long, long);
extern long word_loop(const long *, const long *);
extern long check_saved(long, long);
int main(void) {
    for (long x = -100; x <= 100; ++x) {
        long y = 3 * x - 7;
        if (leaf(x) != x + 3 || padding_leaf(x) != x + 3 || call_only(x) != (x & 1) ||
            choose(x, y) != ((x & 1) ? x + 1 : y) ||
            check_saved(x, y) != ((x & 1) ? x + 1 : y))
            return 1;
    }
    const long value = 123;
    if (word_loop(&value, &value) != 0)
        return 2;
    puts("KEEL_SIZE_OK");
    return 0;
}
