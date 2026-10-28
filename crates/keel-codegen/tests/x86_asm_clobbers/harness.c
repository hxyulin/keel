#include <stdint.h>
#include <stdio.h>

extern uint64_t plain(uint64_t);
extern uint64_t clobber(uint64_t, uint64_t);
extern uint64_t tied_live(uint64_t, uint64_t);
extern uint64_t memory(uint64_t *, uint64_t);
extern uint64_t equal(uint64_t, uint64_t);
extern uint64_t constant_live(uint64_t);
extern int check_saved(uint64_t (*)(uint64_t, uint64_t), uint64_t, uint64_t);

int main(void) {
    const uint64_t values[] = {0, 1, UINT64_MAX, 0x1122334455667788,
                              0x8000000000000000, 0xdeadbeefcafef00d};
    for (unsigned i = 0; i < sizeof(values) / sizeof(values[0]); ++i) {
        uint64_t x = values[i];
        if (plain(x) != x + 7 ||
            constant_live(x) != x + 2 * UINT64_C(0x1122334455667788)) return 1;
        for (unsigned j = 0; j < sizeof(values) / sizeof(values[0]); ++j) {
            uint64_t y = values[j];
            uint64_t words[] = {0xf00dcafe, x, 0xabcdef1234567890};
            if (clobber(x, y) != x + y || tied_live(x, y) != 2 * (x + y) ||
                equal(x, y) != x + y + (x == y) ||
                memory(&words[1], y) != x + 2 * y || words[1] != x + y ||
                words[0] != 0xf00dcafe || words[2] != UINT64_C(0xabcdef1234567890) ||
                !check_saved(clobber, x, y) || !check_saved(tied_live, x, y)) return 1;
        }
    }
    puts("PASS");
    return 0;
}
