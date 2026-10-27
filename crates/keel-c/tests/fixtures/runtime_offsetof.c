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
struct ZeroNestedOffset { struct OffsetLeaf first; };

_Static_assert(__builtin_offsetof(struct ZeroNestedOffset, first.value)
    == __builtin_offsetof(struct OffsetLeaf, value), "nested member at offset zero");

_Static_assert(__builtin_types_compatible_p(
    __typeof__(__builtin_offsetof(struct OffsetRecord, rows[2][1].value)),
    unsigned long), "offsetof has the target size type");
enum { STATIC_OFFSET = __builtin_offsetof(struct OffsetRecord, rows[2][1].value) };
unsigned long static_offset = __builtin_offsetof(struct OffsetRecord, rows[2][1].value);

unsigned long runtime_offset(int i, long long j) {
    return __builtin_offsetof(struct OffsetRecord, rows[i][j].value);
}
unsigned long anonymous_offset(int i) {
    return __builtin_offsetof(struct OffsetRecord, nested[i]);
}
unsigned long flexible_offset(long long i) {
    return __builtin_offsetof(struct OffsetRecord, tail[i]);
}
unsigned long folded_offset(void) { return STATIC_OFFSET + static_offset; }

int offset_index_evaluations;
static __attribute__((noinline)) int next_offset_index(int *i) {
    ++offset_index_evaluations;
    return (*i)++;
}
unsigned long once_offset(int *i) {
    return __builtin_offsetof(struct OffsetRecord, rows[next_offset_index(i)][2].value);
}
unsigned long comma_offset(int *i) {
    return __builtin_offsetof(struct OffsetRecord, rows[((*i)++, 1)][1].value);
}
unsigned long conditional_offset(int *i, int take) {
    return __builtin_offsetof(struct OffsetRecord, rows[take ? (*i)++ : 2][0].value);
}
int unevaluated_offset(int *i) {
    return __builtin_constant_p(__builtin_offsetof(
        struct OffsetRecord, rows[(*i)++][0].value));
}
unsigned long zero_stride_offset(int *i) {
    return __builtin_offsetof(struct ZeroOffset, empty[(*i)++]);
}
