/* Compare traces with Clang: fusion, shared state, and numeric-token fallback. */
#include <stdio.h>
#include <string.h>

static int fused(const unsigned char *code) {
    static const void *const table[4] = {
        [0] = &&add, [1] = &&sub, [1] = &&mul, [2] = &&add, [3] = &&done
    };
    int pc = 0, value = 1, steps = 0, opcode;
#define NEXT() goto *table[opcode = code[pc++]]
    NEXT();
add:
    printf("add %d %d %d\n", pc, opcode, value);
    value += ++steps;
    NEXT();
mul:
    printf("mul %d %d %d\n", pc, opcode, value);
    value *= ++steps;
    NEXT();
sub: /* Address-taken in an overwritten initializer. */
    value -= 99;
    NEXT();
done:
    printf("done %d %d %d %d\n", pc, opcode, value, steps);
    return value;
#undef NEXT
}

static int copied(int n) {
    static const void *const table[] = { &&a, &&b };
    const void *copy[2];
    memcpy(copy, table, sizeof copy);
    goto *copy[n];
a: return 11;
b: return 22;
}

static int offsets(int n) {
    static const long offsets[] = { &&a-&&a, &&b-&&a };
    goto *(&&a + offsets[n]);
a: return 33;
b: return 44;
}

static int changed(int n) {
    static const void *table[] = { &&a, &&b };
    const void *t = table[0]; table[0] = table[1]; table[1] = t;
    goto *table[n];
a: return 55;
b: return 66;
}

static int observed(int n) {
    static void *const volatile table[] = { &&a, &&b };
    goto *table[n];
a: return 77;
b: return 88;
}

int main(void) {
    const unsigned char code[] = { 0, 1, 2, 1, 0, 3 };
    printf("result %d\n", fused(code));
    for (int i = 0; i < 2; ++i)
        printf("fallback %d %d %d %d\n", copied(i), offsets(i), changed(i), observed(i));
    return 0;
}
