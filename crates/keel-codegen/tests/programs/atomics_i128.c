#include <pthread.h>
#include <stdint.h>
#include <stdio.h>

typedef unsigned __int128 u128;
typedef __int128 s128;

u128 k_load_relaxed(u128 *p);
u128 k_load_acquire(u128 *p);
u128 k_load_seq_cst(u128 *p);
void k_store_relaxed(u128 *p, u128 v);
void k_store_release(u128 *p, u128 v);
void k_store_seq_cst(u128 *p, u128 v);
uint8_t k_cas_strong(u128 *p, u128 expected, u128 desired, u128 *old);
uint8_t k_cas_weak(u128 *p, u128 expected, u128 desired, u128 *old);
uint8_t k_cas_release(u128 *p, u128 expected, u128 desired, u128 *old);
uint8_t k_cas_acquire(u128 *p, u128 expected, u128 desired, u128 *old);
u128 k_add(u128 *p, u128 v);
u128 k_sub(u128 *p, u128 v);
u128 k_and(u128 *p, u128 v);
u128 k_or(u128 *p, u128 v);
u128 k_xor(u128 *p, u128 v);
u128 k_xchg(u128 *p, u128 v);
u128 k_umin(u128 *p, u128 v);
u128 k_umax(u128 *p, u128 v);
s128 k_smin(s128 *p, s128 v);
s128 k_smax(s128 *p, s128 v);
void k_bump(u128 *p);

static int failures;

#define CHECK(cond, ...)                                                        \
    do {                                                                       \
        if (!(cond) && failures++ < 20) printf(__VA_ARGS__);                   \
    } while (0)

static u128 wide(uint64_t hi, uint64_t lo) { return (u128)hi << 64 | lo; }

/* The halves cover carries and borrows across 64 bits, and both signs. */
static const uint64_t halves[] = {0, 1, 0x7fffffffffffffff, 0x8000000000000000,
                                  0xffffffffffffffff, 0x0123456789abcdef};
#define H (sizeof halves / sizeof halves[0])

static _Alignas(16) u128 cell;

static void check_values(u128 a, u128 b) {
    u128 old;
    cell = a;
    CHECK(k_load_relaxed(&cell) == a && k_load_acquire(&cell) == a && k_load_seq_cst(&cell) == a,
          "load wrong\n");
    k_store_relaxed(&cell, b);
    CHECK(cell == b, "store relaxed wrong\n");
    k_store_release(&cell, a);
    CHECK(cell == a, "store release wrong\n");
    k_store_seq_cst(&cell, b);
    CHECK(cell == b, "store seq_cst wrong\n");

    uint8_t (*cases[])(u128 *, u128, u128, u128 *) = {k_cas_strong, k_cas_weak, k_cas_release,
                                                       k_cas_acquire};
    for (int c = 0; c < 4; c++) {
        cell = a;
        int tries = 0, flag;
        /* Only the weak one may fail while memory holds the expected value. */
        do flag = cases[c](&cell, a, b, &old);
        while (c == 1 && !flag && old == a && ++tries < 1000);
        CHECK(flag == 1 && old == a && cell == b, "cas %d success wrong\n", c);
        if (a != b) {
            flag = cases[c](&cell, a, a, &old);
            CHECK(flag == 0 && old == b && cell == b, "cas %d failure wrong\n", c);
        }
    }

    struct {
        u128 (*f)(u128 *, u128);
        u128 want;
        const char *name;
    } rmws[] = {
        {k_add, a + b, "add"},
        {k_sub, a - b, "sub"},
        {k_and, a & b, "and"},
        {k_or, a | b, "or"},
        {k_xor, a ^ b, "xor"},
        {k_xchg, b, "xchg"},
        {k_umin, a < b ? a : b, "umin"},
        {k_umax, a > b ? a : b, "umax"},
    };
    for (unsigned r = 0; r < sizeof rmws / sizeof rmws[0]; r++) {
        cell = a;
        old = rmws[r].f(&cell, b);
        CHECK(old == a && cell == rmws[r].want, "%s wrong\n", rmws[r].name);
    }
    s128 sa = (s128)a, sb = (s128)b, sm = sa;
    s128 sold = k_smin(&sm, sb);
    CHECK(sold == sa && sm == (sa < sb ? sa : sb), "smin wrong\n");
    sm = sa;
    sold = k_smax(&sm, sb);
    CHECK(sold == sa && sm == (sa > sb ? sa : sb), "smax wrong\n");
}

#define THREADS 4
#define ROUNDS 100000

static _Alignas(16) u128 pair;
static volatile int done;

static void *bump(void *arg) {
    (void)arg;
    for (int i = 0; i < ROUNDS; i++) k_bump(&pair);
    return 0;
}

/* A torn read would see one half from before an update and one after. */
static void *read_pair(void *arg) {
    (void)arg;
    while (!done) {
        u128 v = k_load_acquire(&pair);
        uint64_t lo = (uint64_t)v, hi = (uint64_t)(v >> 64);
        CHECK(hi == ~lo, "torn read %#llx %#llx\n", (unsigned long long)hi, (unsigned long long)lo);
        v = k_load_seq_cst(&pair);
        lo = (uint64_t)v, hi = (uint64_t)(v >> 64);
        CHECK(hi == ~lo, "torn seq_cst read\n");
    }
    return 0;
}

int main(void) {
    for (unsigned i = 0; i < H; i++)
        for (unsigned j = 0; j < H; j++)
            for (unsigned k = 0; k < H; k += 2)
                check_values(wide(halves[i], halves[j]), wide(halves[k], halves[(i + k) % H]));
    pair = wide(~(uint64_t)0, 0);
    pthread_t writers[THREADS], reader;
    pthread_create(&reader, 0, read_pair, 0);
    for (int i = 0; i < THREADS; i++) pthread_create(&writers[i], 0, bump, 0);
    for (int i = 0; i < THREADS; i++) pthread_join(writers[i], 0);
    done = 1;
    pthread_join(reader, 0);
    CHECK((uint64_t)pair == (uint64_t)THREADS * ROUNDS, "pair counter %llu\n",
          (unsigned long long)(uint64_t)pair);
    /* An exit status is taken mod 256. */
    return failures != 0;
}
