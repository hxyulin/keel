/* Compiled by the host C compiler, never by Keel. Bit positions are independent
 * of C record declarations. Guard pages turn an out-of-object load into a fault. */
#include <stdio.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>

extern unsigned long packed_read(void *, unsigned);
extern void packed_write(void *, unsigned, unsigned long);
extern void packed_initial(unsigned char *, int);
extern long packed_update(void *, int);
extern long packed_result(void *, int, int);
extern unsigned int packed_unsigned_result(void *, int, unsigned int);
static const unsigned start[] = {0, 31, 7, 71, 31, 31, 0, 32, 0, 0};
static const unsigned width[] = {3, 2, 64, 9, 2, 2, 9, 3, 63, 63};
static const unsigned size[] = {1, 5, 11, 11, 5, 6, 2, 5, 8, 8};
static const unsigned sign[] = {1, 0, 0, 1, 0, 0, 1, 0, 0, 1};

static void insert(unsigned char *p, unsigned shift, unsigned bits, unsigned long v) {
    for (unsigned i = 0; i < bits; i++) {
        unsigned bit = shift + i, mask = 1u << (bit % 8);
        p[bit / 8] = (p[bit / 8] & ~mask) | (((v >> i) & 1) * mask);
    }
}
static unsigned long extract(unsigned char *p, unsigned which) {
    unsigned long v = 0;
    for (unsigned i = 0; i < width[which]; i++) {
        unsigned bit = start[which] + i;
        v |= (unsigned long)((p[bit / 8] >> (bit % 8)) & 1) << i;
    }
    if (sign[which] && ((v >> (width[which] - 1)) & 1))
        v |= ~0UL << width[which];
    return v;
}
int main(void) {
    unsigned long page = sysconf(_SC_PAGESIZE);
    unsigned char *mapping = mmap(0, page * 3, PROT_NONE, MAP_PRIVATE | MAP_ANON, -1, 0);
    if (mapping == MAP_FAILED || mprotect(mapping + page, page, PROT_READ | PROT_WRITE)) return 2;
    unsigned long state = 0x8617a539a12905cdUL;
    for (unsigned n = 0; n < 256; n++) {
        state ^= state << 13; state ^= state >> 7; state ^= state << 17;
        for (unsigned k = 0; k < 10; k++) {
            unsigned char actual[32], expected[32];
            for (unsigned i = 0; i < 32; i++) actual[i] = expected[i] = (state >> (i % 8) * 8) ^ i;
            if (packed_read(actual + 3, k) != extract(expected + 3, k)) return 3;
            insert(expected + 3, start[k], width[k], state);
            packed_write(actual + 3, k, state);
            if (memcmp(actual, expected, 32) || packed_read(actual + 3, k) != extract(expected + 3, k)) return 4;
            for (unsigned end = 0; end < 2; end++) {
                unsigned char *p = mapping + page + (end ? page - size[k] : 0);
                memcpy(p, expected + 3, size[k]);
                if (packed_read(p, k) != extract(expected + 3, k)) return 5;
                packed_write(p, k, state);
                if (memcmp(p, expected + 3, size[k])) return 6;
            }
        }
    }
    unsigned char expected[11] = {0}, actual[11];
    insert(expected, 0, 7, 85);
    insert(expected, 7, 64, 0xfedcba9876543210UL);
    insert(expected, 71, 9, -129UL);
    expected[10] = 53;
    for (int local = 0; local < 2; local++) {
        packed_initial(actual, local);
        if (memcmp(expected, actual, 11)) return 7;
    }
    insert(expected, 71, 9, -20UL);
    memcpy(actual, expected, 11);
    if (packed_update(actual, 3) != -16) return 8;
    insert(expected, 71, 9, -16UL);
    if (memcmp(expected, actual, 11)) return 9;
    for (int op = 0; op < 6; op++) {
        for (int old = -256; old <= 255; old += 73) {
            for (int delta = -700; delta < 700; delta += 71) {
                insert(expected, 71, 9, old);
                memcpy(actual, expected, 11);
                int next = op == 0 ? delta : op == 1 ? old + delta : op < 4 ? old + 1 : old - 1;
                next = (next & 511) - ((next & 256) ? 512 : 0);
                long result = (op == 3 || op == 5) ? old : next;
                if (packed_result(actual, op, delta) != result) return 10;
                insert(expected, 71, 9, next);
                if (memcmp(expected, actual, 11)) return 11;
            }
        }
    }
    for (unsigned op = 0; op < 6; op++) {
        for (unsigned old = 0; old < 4; old++) {
            for (unsigned delta = 0; delta < 1024; delta += 41) {
                insert(expected, 31, 2, old);
                memcpy(actual, expected, 11);
                unsigned next = (op == 0 ? delta : op == 1 ? old + delta : op < 4 ? old + 1 : old - 1) & 3;
                unsigned result = (op == 3 || op == 5) ? old : next;
                if (packed_unsigned_result(actual, op, delta) != result) return 12;
                insert(expected, 31, 2, next);
                if (memcmp(expected, actual, 11)) return 13;
            }
        }
    }
    munmap(mapping, page * 3);
    puts("PASS");
    return 0;
}
