#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

struct OffsetLeaf { short first; unsigned long value; };
struct OffsetRecord {
    char tag;
    struct OffsetLeaf rows[4][3];
    union {
        unsigned long alias;
        struct { char pad; int nested[5]; };
    };
    unsigned long tail[];
};
struct ZeroOffset { int prefix; struct {} empty[1]; };

extern unsigned long runtime_offset(int, long long), anonymous_offset(int);
extern unsigned long flexible_offset(long long), folded_offset(void);
extern unsigned long once_offset(int *), comma_offset(int *), conditional_offset(int *, int);
extern unsigned long zero_stride_offset(int *);
extern int unevaluated_offset(int *), offset_index_evaluations;

static int mismatch(const char *name, unsigned long got, unsigned long want) {
    if (got == want) return 0;
    fprintf(stderr, "%s: got %lu expected %lu\n", name, got, want);
    return 1;
}

int main(void) {
    struct OffsetRecord object;
    unsigned long first = (char *)&object.rows[0][0].value - (char *)&object;
    unsigned long nested = (char *)&object.nested[0] - (char *)&object;
    unsigned long tail = (char *)&object.tail - (char *)&object;
    for (int i = 0; i < 4; ++i) {
        for (int j = 0; j < 3; ++j) {
            unsigned long want = (char *)&object.rows[i][j].value - (char *)&object;
            if (mismatch("actual member address", runtime_offset(i, j), want)) return 1;
        }
    }
    const long long indices[] = { -17, -1, 0, 1, 19, 0x100000003LL, INT64_MAX, INT64_MIN };
    for (unsigned k = 0; k < sizeof(indices)/sizeof(indices[0]); ++k) {
        long long j = indices[k];
        for (int i = -7; i < 8; ++i) {
            unsigned long want = first + (unsigned long)i * sizeof(object.rows[0])
                + (unsigned long)j * sizeof(object.rows[0][0]);
            if (mismatch("unsigned offset arithmetic", runtime_offset(i, j), want)) return 2;
        }
        if (mismatch("flexible array", flexible_offset(j), tail + (unsigned long)j * sizeof(unsigned long))) return 3;
    }
    for (int i = -5; i < 9; ++i) {
        if (mismatch("anonymous nesting", anonymous_offset(i), nested + (unsigned long)i * sizeof(int))) return 4;
    }
    if (mismatch("static initializer and enum", folded_offset(), 2 * (first + 2*sizeof(object.rows[0]) + sizeof(struct OffsetLeaf)))) return 5;
    int i = 1;
    if (mismatch("index function", once_offset(&i), first + sizeof(object.rows[0]) + 2*sizeof(struct OffsetLeaf))
        || i != 2 || offset_index_evaluations != 1) return 6;
    if (mismatch("comma side effect", comma_offset(&i), first + sizeof(object.rows[0]) + sizeof(struct OffsetLeaf)) || i != 3) return 7;
    if (mismatch("conditional side effect", conditional_offset(&i, 1), first + 3*sizeof(object.rows[0])) || i != 4) return 8;
    if (mismatch("untaken conditional", conditional_offset(&i, 0), first + 2*sizeof(object.rows[0])) || i != 4) return 9;
#ifndef OFFSETOF_REFERENCE
    /* Clang lifts offsetof-index side effects out of constant_p. Keel keeps
       the builtin's documented unevaluated-operand contract. */
    if (unevaluated_offset(&i) != 0 || i != 4) return 10;
#endif
    if (mismatch("zero stride", zero_stride_offset(&i), offsetof(struct ZeroOffset, empty)) || i != 5) return 11;
    puts("runtime-offsetof PASS");
    return 0;
}
