/* GNU C extensions, C11 atomics and _Generic. */
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>

/* Computed goto: a small threaded interpreter, with its dispatch table
   in static storage, and label differences as offsets. */
static int run(const unsigned char *code) {
    static const void *const table[] = { &&op_push, &&op_add, &&op_mul, &&op_halt };
    int stack[16], sp = 0, pc = 0;
#define DISPATCH() goto *table[code[pc++]]
    DISPATCH();
op_push:
    stack[sp++] = code[pc++];
    DISPATCH();
op_add:
    sp--;
    stack[sp - 1] += stack[sp];
    DISPATCH();
op_mul:
    sp--;
    stack[sp - 1] *= stack[sp];
    DISPATCH();
op_halt:
    return stack[sp - 1];
}

static int by_offset(int i) {
    static const int offsets[] = { &&zero - &&zero, &&one - &&zero, &&two - &&zero };
    goto *(&&zero + offsets[i]);
zero:
    return 100;
one:
    return 101;
two:
    return 102;
}

static int classify(int c) {
    switch (c) {
    case '0' ... '9':
        return 1;
    case 'a' ... 'z':
    case 'A' ... 'Z':
        return 2;
    default:
        return 0;
    }
}

#define MAX(a, b) ({ __typeof__(a) _a = (a); __typeof__(b) _b = (b); _a > _b ? _a : _b; })

static int calls;
static int next(void) { return ++calls; }

#define TYPE_NAME(x) _Generic((x), int: "int", unsigned long: "unsigned long", double: "double", char *: "char *", default: "other")

struct counted { int n; atomic_int refs; };

static unsigned __int128 wide(uint64_t a) {
    /* A u64 constant above INT64_MAX converted to __int128. */
    return (unsigned __int128)a * 17014118346046923173ULL;
}

int main(void) {
    const unsigned char prog[] = { 0, 6, 0, 7, 2, 0, 3, 1, 3 };
    printf("run %d\n", run(prog));
    for (int i = 0; i < 3; i++)
        printf("offset %d\n", by_offset(i));
    printf("classify %d %d %d %d\n", classify('5'), classify('q'), classify('Q'), classify('?'));

    int x = MAX(3, next()), y = MAX(next(), 10);
    printf("max %d %d calls %d\n", x, y, calls);
    int z = next() ?: 99, w = (calls - calls) ?: 42;
    printf("elvis %d %d calls %d\n", z, w, calls);
    int s = ({ int t = 0; for (int i = 1; i <= 4; i++) t += i * i; t; });
    printf("stmt-expr %d\n", s);

    unsigned long ul = 1;
    char buf[2];
    printf("generic %s %s %s %s %s\n", TYPE_NAME(1), TYPE_NAME(ul), TYPE_NAME(1.5), TYPE_NAME(buf),
           TYPE_NAME('c' + 0L));
    printf("compatible %d %d\n", __builtin_types_compatible_p(int, int),
           __builtin_types_compatible_p(int, long));
    printf("choose %d\n", __builtin_choose_expr(sizeof(int) == 4, 1, 2.0));

    int v = 5;
    int old = __sync_fetch_and_add(&v, 3);
    int now = __sync_add_and_fetch(&v, 1);
    int swapped = __sync_bool_compare_and_swap(&v, 9, 20);
    int seen = __sync_val_compare_and_swap(&v, 1, 2);
    __sync_synchronize();
    printf("sync %d %d %d %d %d\n", old, now, swapped, seen, v);

    long l = 10;
    long prev = __atomic_fetch_sub(&l, 4, __ATOMIC_ACQ_REL);
    long expected = 6;
    int ok = __atomic_compare_exchange_n(&l, &expected, 60, 0, __ATOMIC_SEQ_CST, __ATOMIC_RELAXED);
    expected = 7;
    int bad = __atomic_compare_exchange_n(&l, &expected, 70, 0, __ATOMIC_SEQ_CST, __ATOMIC_RELAXED);
    printf("atomic %ld %d %d %ld %ld %ld\n", prev, ok, bad, expected, __atomic_load_n(&l, __ATOMIC_ACQUIRE),
           __atomic_exchange_n(&l, 1, __ATOMIC_SEQ_CST));
    unsigned char flag = 0;
    int was = __atomic_test_and_set(&flag, __ATOMIC_SEQ_CST);
    int again = __atomic_test_and_set(&flag, __ATOMIC_SEQ_CST);
    __atomic_clear(&flag, __ATOMIC_RELEASE);
    printf("flag %d %d %d\n", was, again, flag);

    struct counted c = { 1, 2 };
    atomic_fetch_add(&c.refs, 5);
    c.refs += 3;
    c.refs++;
    int before = c.refs--;
    _Atomic(long) al = 100;
    al -= 1;
    int cmp = 11;
    atomic_int ai = ATOMIC_VAR_INIT(11);
    int exchanged = atomic_compare_exchange_strong(&ai, &cmp, 12);
    printf("stdatomic %d %d %ld %d %d %d\n", (int)c.refs, before, (long)al, exchanged, atomic_load(&ai),
           atomic_is_lock_free(&ai));
    atomic_flag f = ATOMIC_FLAG_INIT;
    printf("atomic_flag %d %d\n", atomic_flag_test_and_set(&f), atomic_flag_test_and_set(&f));

    unsigned __int128 r = wide(3);
    printf("int128 %016llx%016llx\n", (unsigned long long)(r >> 64), (unsigned long long)r);
    printf("frame %d\n", __builtin_frame_address(0) != 0);
    static const int squares[8] = { [0 ... 3] = 1, [4 ... 7] = 2 };
    for (int i = 0; i < 8; i++)
        printf("%d", squares[i]);
    printf("\n");
    return __extension__ 0;
}
