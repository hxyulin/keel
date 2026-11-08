#include <stdint.h>
#include <stdio.h>

extern int64_t max_plus_seven(int64_t a, int64_t b);

int main(void) {
    const int64_t cases[][3] = {
        { 12, 5, 19 }, { 5, 12, 19 }, { -12, -5, 2 },
        { -5, -12, 2 }, { 0, 0, 7 }, { INT64_MIN, -7, 0 },
        { INT64_MAX - 7, INT64_MIN, INT64_MAX }
    };
    for (unsigned i = 0; i < sizeof(cases) / sizeof(cases[0]); ++i) {
        int64_t actual = max_plus_seven(cases[i][0], cases[i][1]);
        if (actual != cases[i][2]) {
            fprintf(stderr, "case %u failed\n", i);
            return 1;
        }
    }
    return 0;
}
