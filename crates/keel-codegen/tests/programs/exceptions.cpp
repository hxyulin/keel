#include <cstdio>
#include <stdexcept>

extern "C" {
#include "clobber.h"
}

extern "C" int k_catch(int x);
extern "C" int k_cleanup(int x);
extern "C" int k_two(int a, int b, int c);
extern "C" int cleanups;

/* Clobbers every caller-saved register first, as any call may, so keel
   values live across an invoke must be in callee-saved ones. */
extern "C" int c_may_throw(int x) {
    clobber();
    if (x < 0) throw -x;
    if (x == 1000) throw std::logic_error("not an int");
    return 2 * x;
}

static int failures;

static void check(const char *what, int got, int want) {
    if (got != want) {
        printf("%s = %d, want %d\n", what, got, want);
        failures++;
    }
}

int main() {
    check("k_catch(5)", k_catch(5), 11);
    check("k_catch(-3)", k_catch(-3), 3 + 1000 + (-3 + 7));
    check("k_catch(1000)", k_catch(1000), -1);

    check("k_cleanup(4)", k_cleanup(4), 8);
    try {
        k_cleanup(-9);
        check("k_cleanup(-9) returned", 1, 0);
    } catch (int e) {
        check("k_cleanup(-9) threw", e, 9);
    }
    check("cleanups", cleanups, -9 + 100);

    check("k_two(1, 2, 3)", k_two(1, 2, 3), 2 + 4 + 6);
    check("k_two(-1, 2, 3)", k_two(-1, 2, 3), 100);
    check("k_two(1, 2, -3)", k_two(1, 2, -3), 200);
    try {
        k_two(1, -2, 3);
        check("k_two(1, -2, 3) returned", 1, 0);
    } catch (int e) {
        check("k_two(1, -2, 3) threw", e, 2);
    }
    /* An exit status is taken mod 256. */
    return failures != 0;
}
