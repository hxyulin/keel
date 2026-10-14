#include <pthread.h>
#include <stdint.h>
#include <stdio.h>

uint8_t k_add_i8(uint8_t *p, uint8_t v);
uint8_t k_sub_i8(uint8_t *p, uint8_t v);
uint8_t k_and_i8(uint8_t *p, uint8_t v);
uint8_t k_or_i8(uint8_t *p, uint8_t v);
uint8_t k_xor_i8(uint8_t *p, uint8_t v);
uint8_t k_xchg_i8(uint8_t *p, uint8_t v);
uint8_t k_umin_i8(uint8_t *p, uint8_t v);
uint8_t k_umax_i8(uint8_t *p, uint8_t v);
int8_t k_smin_i8(int8_t *p, int8_t v);
int8_t k_smax_i8(int8_t *p, int8_t v);
uint8_t k_cas_i8(uint8_t *p, uint8_t expected, uint8_t new_value);
uint8_t k_swap_i8(uint8_t *p, uint8_t v);
uint16_t k_add_i16(uint16_t *p, uint16_t v);
uint16_t k_sub_i16(uint16_t *p, uint16_t v);
uint16_t k_and_i16(uint16_t *p, uint16_t v);
uint16_t k_or_i16(uint16_t *p, uint16_t v);
uint16_t k_xor_i16(uint16_t *p, uint16_t v);
uint16_t k_xchg_i16(uint16_t *p, uint16_t v);
uint16_t k_umin_i16(uint16_t *p, uint16_t v);
uint16_t k_umax_i16(uint16_t *p, uint16_t v);
int16_t k_smin_i16(int16_t *p, int16_t v);
int16_t k_smax_i16(int16_t *p, int16_t v);
uint16_t k_cas_i16(uint16_t *p, uint16_t expected, uint16_t new_value);
uint16_t k_swap_i16(uint16_t *p, uint16_t v);
uint32_t k_add_i32(uint32_t *p, uint32_t v);
uint32_t k_sub_i32(uint32_t *p, uint32_t v);
uint32_t k_and_i32(uint32_t *p, uint32_t v);
uint32_t k_or_i32(uint32_t *p, uint32_t v);
uint32_t k_xor_i32(uint32_t *p, uint32_t v);
uint32_t k_xchg_i32(uint32_t *p, uint32_t v);
uint32_t k_umin_i32(uint32_t *p, uint32_t v);
uint32_t k_umax_i32(uint32_t *p, uint32_t v);
int32_t k_smin_i32(int32_t *p, int32_t v);
int32_t k_smax_i32(int32_t *p, int32_t v);
uint32_t k_cas_i32(uint32_t *p, uint32_t expected, uint32_t new_value);
uint32_t k_swap_i32(uint32_t *p, uint32_t v);
uint64_t k_add_i64(uint64_t *p, uint64_t v);
uint64_t k_sub_i64(uint64_t *p, uint64_t v);
uint64_t k_and_i64(uint64_t *p, uint64_t v);
uint64_t k_or_i64(uint64_t *p, uint64_t v);
uint64_t k_xor_i64(uint64_t *p, uint64_t v);
uint64_t k_xchg_i64(uint64_t *p, uint64_t v);
uint64_t k_umin_i64(uint64_t *p, uint64_t v);
uint64_t k_umax_i64(uint64_t *p, uint64_t v);
int64_t k_smin_i64(int64_t *p, int64_t v);
int64_t k_smax_i64(int64_t *p, int64_t v);
uint64_t k_cas_i64(uint64_t *p, uint64_t expected, uint64_t new_value);
uint64_t k_swap_i64(uint64_t *p, uint64_t v);
void *k_xchg_ptr(void **p, void *v);
void k_count(int64_t *p, int64_t n);
void k_lock(int32_t *l);
void k_unlock(int32_t *l);

static int failures;

static const uint64_t values[] = {0, 1, 2, 0x7f, 0x80, 0xff, 0x7fff, 0x8000, 0xffff,
    0x7fffffff, 0x80000000, 0xffffffff, 0x7fffffffffffffff, 0x8000000000000000,
    0xffffffffffffffff, 0x123456789abcdef0};
#define N (sizeof values / sizeof values[0])

static void check_i8(void) {
    for (unsigned i = 0; i < N; i++)
        for (unsigned j = 0; j < N; j++) {
            {
                uint8_t a = (uint8_t)values[i], b = (uint8_t)values[j], m = a;
                uint8_t old = k_add_i8(&m, b);
                uint8_t want = (uint8_t)(a + b);
                if (old != a || m != want) {
                    if (failures++ < 20) printf("add.i8(%#llx, %#llx): old %#llx new %#llx\n",
                        (unsigned long long)a, (unsigned long long)b, (unsigned long long)old, (unsigned long long)m);
                }
            }
            {
                uint8_t a = (uint8_t)values[i], b = (uint8_t)values[j], m = a;
                uint8_t old = k_sub_i8(&m, b);
                uint8_t want = (uint8_t)(a - b);
                if (old != a || m != want) {
                    if (failures++ < 20) printf("sub.i8(%#llx, %#llx): old %#llx new %#llx\n",
                        (unsigned long long)a, (unsigned long long)b, (unsigned long long)old, (unsigned long long)m);
                }
            }
            {
                uint8_t a = (uint8_t)values[i], b = (uint8_t)values[j], m = a;
                uint8_t old = k_and_i8(&m, b);
                uint8_t want = (uint8_t)(a & b);
                if (old != a || m != want) {
                    if (failures++ < 20) printf("and.i8(%#llx, %#llx): old %#llx new %#llx\n",
                        (unsigned long long)a, (unsigned long long)b, (unsigned long long)old, (unsigned long long)m);
                }
            }
            {
                uint8_t a = (uint8_t)values[i], b = (uint8_t)values[j], m = a;
                uint8_t old = k_or_i8(&m, b);
                uint8_t want = (uint8_t)(a | b);
                if (old != a || m != want) {
                    if (failures++ < 20) printf("or.i8(%#llx, %#llx): old %#llx new %#llx\n",
                        (unsigned long long)a, (unsigned long long)b, (unsigned long long)old, (unsigned long long)m);
                }
            }
            {
                uint8_t a = (uint8_t)values[i], b = (uint8_t)values[j], m = a;
                uint8_t old = k_xor_i8(&m, b);
                uint8_t want = (uint8_t)(a ^ b);
                if (old != a || m != want) {
                    if (failures++ < 20) printf("xor.i8(%#llx, %#llx): old %#llx new %#llx\n",
                        (unsigned long long)a, (unsigned long long)b, (unsigned long long)old, (unsigned long long)m);
                }
            }
            {
                uint8_t a = (uint8_t)values[i], b = (uint8_t)values[j], m = a;
                uint8_t old = k_xchg_i8(&m, b);
                uint8_t want = (uint8_t)(b);
                if (old != a || m != want) {
                    if (failures++ < 20) printf("xchg.i8(%#llx, %#llx): old %#llx new %#llx\n",
                        (unsigned long long)a, (unsigned long long)b, (unsigned long long)old, (unsigned long long)m);
                }
            }
            {
                uint8_t a = (uint8_t)values[i], b = (uint8_t)values[j], m = a;
                uint8_t old = k_umin_i8(&m, b);
                uint8_t want = (uint8_t)(a < b ? a : b);
                if (old != a || m != want) {
                    if (failures++ < 20) printf("umin.i8(%#llx, %#llx): old %#llx new %#llx\n",
                        (unsigned long long)a, (unsigned long long)b, (unsigned long long)old, (unsigned long long)m);
                }
            }
            {
                uint8_t a = (uint8_t)values[i], b = (uint8_t)values[j], m = a;
                uint8_t old = k_umax_i8(&m, b);
                uint8_t want = (uint8_t)(a > b ? a : b);
                if (old != a || m != want) {
                    if (failures++ < 20) printf("umax.i8(%#llx, %#llx): old %#llx new %#llx\n",
                        (unsigned long long)a, (unsigned long long)b, (unsigned long long)old, (unsigned long long)m);
                }
            }
            {
                int8_t a = (int8_t)values[i], b = (int8_t)values[j], m = a;
                int8_t old = k_smin_i8(&m, b);
                int8_t want = (int8_t)(a < b ? a : b);
                if (old != a || m != want) {
                    if (failures++ < 20) printf("smin.i8(%#llx, %#llx): old %#llx new %#llx\n",
                        (unsigned long long)a, (unsigned long long)b, (unsigned long long)old, (unsigned long long)m);
                }
            }
            {
                int8_t a = (int8_t)values[i], b = (int8_t)values[j], m = a;
                int8_t old = k_smax_i8(&m, b);
                int8_t want = (int8_t)(a > b ? a : b);
                if (old != a || m != want) {
                    if (failures++ < 20) printf("smax.i8(%#llx, %#llx): old %#llx new %#llx\n",
                        (unsigned long long)a, (unsigned long long)b, (unsigned long long)old, (unsigned long long)m);
                }
            }
            {
                uint8_t a = (uint8_t)values[i], b = (uint8_t)values[j], m = a;
                /* Succeeds: expected matches. */
                uint8_t old = k_cas_i8(&m, a, b);
                if (old != a || m != b) { failures++; printf("cas.i8 success wrong\n"); }
                /* Fails unless a == b: expected is a, memory holds b. */
                old = k_cas_i8(&m, a, (uint8_t)(a + 1));
                uint8_t want = a == b ? (uint8_t)(a + 1) : b;
                if (old != b || m != want) { failures++; printf("cas.i8 failure wrong\n"); }
                m = a;
                old = k_swap_i8(&m, b);
                if (old != a || m != b) { failures++; printf("swap.i8 wrong\n"); }
            }
        }
}

static void check_i16(void) {
    for (unsigned i = 0; i < N; i++)
        for (unsigned j = 0; j < N; j++) {
            {
                uint16_t a = (uint16_t)values[i], b = (uint16_t)values[j], m = a;
                uint16_t old = k_add_i16(&m, b);
                uint16_t want = (uint16_t)(a + b);
                if (old != a || m != want) {
                    if (failures++ < 20) printf("add.i16(%#llx, %#llx): old %#llx new %#llx\n",
                        (unsigned long long)a, (unsigned long long)b, (unsigned long long)old, (unsigned long long)m);
                }
            }
            {
                uint16_t a = (uint16_t)values[i], b = (uint16_t)values[j], m = a;
                uint16_t old = k_sub_i16(&m, b);
                uint16_t want = (uint16_t)(a - b);
                if (old != a || m != want) {
                    if (failures++ < 20) printf("sub.i16(%#llx, %#llx): old %#llx new %#llx\n",
                        (unsigned long long)a, (unsigned long long)b, (unsigned long long)old, (unsigned long long)m);
                }
            }
            {
                uint16_t a = (uint16_t)values[i], b = (uint16_t)values[j], m = a;
                uint16_t old = k_and_i16(&m, b);
                uint16_t want = (uint16_t)(a & b);
                if (old != a || m != want) {
                    if (failures++ < 20) printf("and.i16(%#llx, %#llx): old %#llx new %#llx\n",
                        (unsigned long long)a, (unsigned long long)b, (unsigned long long)old, (unsigned long long)m);
                }
            }
            {
                uint16_t a = (uint16_t)values[i], b = (uint16_t)values[j], m = a;
                uint16_t old = k_or_i16(&m, b);
                uint16_t want = (uint16_t)(a | b);
                if (old != a || m != want) {
                    if (failures++ < 20) printf("or.i16(%#llx, %#llx): old %#llx new %#llx\n",
                        (unsigned long long)a, (unsigned long long)b, (unsigned long long)old, (unsigned long long)m);
                }
            }
            {
                uint16_t a = (uint16_t)values[i], b = (uint16_t)values[j], m = a;
                uint16_t old = k_xor_i16(&m, b);
                uint16_t want = (uint16_t)(a ^ b);
                if (old != a || m != want) {
                    if (failures++ < 20) printf("xor.i16(%#llx, %#llx): old %#llx new %#llx\n",
                        (unsigned long long)a, (unsigned long long)b, (unsigned long long)old, (unsigned long long)m);
                }
            }
            {
                uint16_t a = (uint16_t)values[i], b = (uint16_t)values[j], m = a;
                uint16_t old = k_xchg_i16(&m, b);
                uint16_t want = (uint16_t)(b);
                if (old != a || m != want) {
                    if (failures++ < 20) printf("xchg.i16(%#llx, %#llx): old %#llx new %#llx\n",
                        (unsigned long long)a, (unsigned long long)b, (unsigned long long)old, (unsigned long long)m);
                }
            }
            {
                uint16_t a = (uint16_t)values[i], b = (uint16_t)values[j], m = a;
                uint16_t old = k_umin_i16(&m, b);
                uint16_t want = (uint16_t)(a < b ? a : b);
                if (old != a || m != want) {
                    if (failures++ < 20) printf("umin.i16(%#llx, %#llx): old %#llx new %#llx\n",
                        (unsigned long long)a, (unsigned long long)b, (unsigned long long)old, (unsigned long long)m);
                }
            }
            {
                uint16_t a = (uint16_t)values[i], b = (uint16_t)values[j], m = a;
                uint16_t old = k_umax_i16(&m, b);
                uint16_t want = (uint16_t)(a > b ? a : b);
                if (old != a || m != want) {
                    if (failures++ < 20) printf("umax.i16(%#llx, %#llx): old %#llx new %#llx\n",
                        (unsigned long long)a, (unsigned long long)b, (unsigned long long)old, (unsigned long long)m);
                }
            }
            {
                int16_t a = (int16_t)values[i], b = (int16_t)values[j], m = a;
                int16_t old = k_smin_i16(&m, b);
                int16_t want = (int16_t)(a < b ? a : b);
                if (old != a || m != want) {
                    if (failures++ < 20) printf("smin.i16(%#llx, %#llx): old %#llx new %#llx\n",
                        (unsigned long long)a, (unsigned long long)b, (unsigned long long)old, (unsigned long long)m);
                }
            }
            {
                int16_t a = (int16_t)values[i], b = (int16_t)values[j], m = a;
                int16_t old = k_smax_i16(&m, b);
                int16_t want = (int16_t)(a > b ? a : b);
                if (old != a || m != want) {
                    if (failures++ < 20) printf("smax.i16(%#llx, %#llx): old %#llx new %#llx\n",
                        (unsigned long long)a, (unsigned long long)b, (unsigned long long)old, (unsigned long long)m);
                }
            }
            {
                uint16_t a = (uint16_t)values[i], b = (uint16_t)values[j], m = a;
                /* Succeeds: expected matches. */
                uint16_t old = k_cas_i16(&m, a, b);
                if (old != a || m != b) { failures++; printf("cas.i16 success wrong\n"); }
                /* Fails unless a == b: expected is a, memory holds b. */
                old = k_cas_i16(&m, a, (uint16_t)(a + 1));
                uint16_t want = a == b ? (uint16_t)(a + 1) : b;
                if (old != b || m != want) { failures++; printf("cas.i16 failure wrong\n"); }
                m = a;
                old = k_swap_i16(&m, b);
                if (old != a || m != b) { failures++; printf("swap.i16 wrong\n"); }
            }
        }
}

static void check_i32(void) {
    for (unsigned i = 0; i < N; i++)
        for (unsigned j = 0; j < N; j++) {
            {
                uint32_t a = (uint32_t)values[i], b = (uint32_t)values[j], m = a;
                uint32_t old = k_add_i32(&m, b);
                uint32_t want = (uint32_t)(a + b);
                if (old != a || m != want) {
                    if (failures++ < 20) printf("add.i32(%#llx, %#llx): old %#llx new %#llx\n",
                        (unsigned long long)a, (unsigned long long)b, (unsigned long long)old, (unsigned long long)m);
                }
            }
            {
                uint32_t a = (uint32_t)values[i], b = (uint32_t)values[j], m = a;
                uint32_t old = k_sub_i32(&m, b);
                uint32_t want = (uint32_t)(a - b);
                if (old != a || m != want) {
                    if (failures++ < 20) printf("sub.i32(%#llx, %#llx): old %#llx new %#llx\n",
                        (unsigned long long)a, (unsigned long long)b, (unsigned long long)old, (unsigned long long)m);
                }
            }
            {
                uint32_t a = (uint32_t)values[i], b = (uint32_t)values[j], m = a;
                uint32_t old = k_and_i32(&m, b);
                uint32_t want = (uint32_t)(a & b);
                if (old != a || m != want) {
                    if (failures++ < 20) printf("and.i32(%#llx, %#llx): old %#llx new %#llx\n",
                        (unsigned long long)a, (unsigned long long)b, (unsigned long long)old, (unsigned long long)m);
                }
            }
            {
                uint32_t a = (uint32_t)values[i], b = (uint32_t)values[j], m = a;
                uint32_t old = k_or_i32(&m, b);
                uint32_t want = (uint32_t)(a | b);
                if (old != a || m != want) {
                    if (failures++ < 20) printf("or.i32(%#llx, %#llx): old %#llx new %#llx\n",
                        (unsigned long long)a, (unsigned long long)b, (unsigned long long)old, (unsigned long long)m);
                }
            }
            {
                uint32_t a = (uint32_t)values[i], b = (uint32_t)values[j], m = a;
                uint32_t old = k_xor_i32(&m, b);
                uint32_t want = (uint32_t)(a ^ b);
                if (old != a || m != want) {
                    if (failures++ < 20) printf("xor.i32(%#llx, %#llx): old %#llx new %#llx\n",
                        (unsigned long long)a, (unsigned long long)b, (unsigned long long)old, (unsigned long long)m);
                }
            }
            {
                uint32_t a = (uint32_t)values[i], b = (uint32_t)values[j], m = a;
                uint32_t old = k_xchg_i32(&m, b);
                uint32_t want = (uint32_t)(b);
                if (old != a || m != want) {
                    if (failures++ < 20) printf("xchg.i32(%#llx, %#llx): old %#llx new %#llx\n",
                        (unsigned long long)a, (unsigned long long)b, (unsigned long long)old, (unsigned long long)m);
                }
            }
            {
                uint32_t a = (uint32_t)values[i], b = (uint32_t)values[j], m = a;
                uint32_t old = k_umin_i32(&m, b);
                uint32_t want = (uint32_t)(a < b ? a : b);
                if (old != a || m != want) {
                    if (failures++ < 20) printf("umin.i32(%#llx, %#llx): old %#llx new %#llx\n",
                        (unsigned long long)a, (unsigned long long)b, (unsigned long long)old, (unsigned long long)m);
                }
            }
            {
                uint32_t a = (uint32_t)values[i], b = (uint32_t)values[j], m = a;
                uint32_t old = k_umax_i32(&m, b);
                uint32_t want = (uint32_t)(a > b ? a : b);
                if (old != a || m != want) {
                    if (failures++ < 20) printf("umax.i32(%#llx, %#llx): old %#llx new %#llx\n",
                        (unsigned long long)a, (unsigned long long)b, (unsigned long long)old, (unsigned long long)m);
                }
            }
            {
                int32_t a = (int32_t)values[i], b = (int32_t)values[j], m = a;
                int32_t old = k_smin_i32(&m, b);
                int32_t want = (int32_t)(a < b ? a : b);
                if (old != a || m != want) {
                    if (failures++ < 20) printf("smin.i32(%#llx, %#llx): old %#llx new %#llx\n",
                        (unsigned long long)a, (unsigned long long)b, (unsigned long long)old, (unsigned long long)m);
                }
            }
            {
                int32_t a = (int32_t)values[i], b = (int32_t)values[j], m = a;
                int32_t old = k_smax_i32(&m, b);
                int32_t want = (int32_t)(a > b ? a : b);
                if (old != a || m != want) {
                    if (failures++ < 20) printf("smax.i32(%#llx, %#llx): old %#llx new %#llx\n",
                        (unsigned long long)a, (unsigned long long)b, (unsigned long long)old, (unsigned long long)m);
                }
            }
            {
                uint32_t a = (uint32_t)values[i], b = (uint32_t)values[j], m = a;
                /* Succeeds: expected matches. */
                uint32_t old = k_cas_i32(&m, a, b);
                if (old != a || m != b) { failures++; printf("cas.i32 success wrong\n"); }
                /* Fails unless a == b: expected is a, memory holds b. */
                old = k_cas_i32(&m, a, (uint32_t)(a + 1));
                uint32_t want = a == b ? (uint32_t)(a + 1) : b;
                if (old != b || m != want) { failures++; printf("cas.i32 failure wrong\n"); }
                m = a;
                old = k_swap_i32(&m, b);
                if (old != a || m != b) { failures++; printf("swap.i32 wrong\n"); }
            }
        }
}

static void check_i64(void) {
    for (unsigned i = 0; i < N; i++)
        for (unsigned j = 0; j < N; j++) {
            {
                uint64_t a = (uint64_t)values[i], b = (uint64_t)values[j], m = a;
                uint64_t old = k_add_i64(&m, b);
                uint64_t want = (uint64_t)(a + b);
                if (old != a || m != want) {
                    if (failures++ < 20) printf("add.i64(%#llx, %#llx): old %#llx new %#llx\n",
                        (unsigned long long)a, (unsigned long long)b, (unsigned long long)old, (unsigned long long)m);
                }
            }
            {
                uint64_t a = (uint64_t)values[i], b = (uint64_t)values[j], m = a;
                uint64_t old = k_sub_i64(&m, b);
                uint64_t want = (uint64_t)(a - b);
                if (old != a || m != want) {
                    if (failures++ < 20) printf("sub.i64(%#llx, %#llx): old %#llx new %#llx\n",
                        (unsigned long long)a, (unsigned long long)b, (unsigned long long)old, (unsigned long long)m);
                }
            }
            {
                uint64_t a = (uint64_t)values[i], b = (uint64_t)values[j], m = a;
                uint64_t old = k_and_i64(&m, b);
                uint64_t want = (uint64_t)(a & b);
                if (old != a || m != want) {
                    if (failures++ < 20) printf("and.i64(%#llx, %#llx): old %#llx new %#llx\n",
                        (unsigned long long)a, (unsigned long long)b, (unsigned long long)old, (unsigned long long)m);
                }
            }
            {
                uint64_t a = (uint64_t)values[i], b = (uint64_t)values[j], m = a;
                uint64_t old = k_or_i64(&m, b);
                uint64_t want = (uint64_t)(a | b);
                if (old != a || m != want) {
                    if (failures++ < 20) printf("or.i64(%#llx, %#llx): old %#llx new %#llx\n",
                        (unsigned long long)a, (unsigned long long)b, (unsigned long long)old, (unsigned long long)m);
                }
            }
            {
                uint64_t a = (uint64_t)values[i], b = (uint64_t)values[j], m = a;
                uint64_t old = k_xor_i64(&m, b);
                uint64_t want = (uint64_t)(a ^ b);
                if (old != a || m != want) {
                    if (failures++ < 20) printf("xor.i64(%#llx, %#llx): old %#llx new %#llx\n",
                        (unsigned long long)a, (unsigned long long)b, (unsigned long long)old, (unsigned long long)m);
                }
            }
            {
                uint64_t a = (uint64_t)values[i], b = (uint64_t)values[j], m = a;
                uint64_t old = k_xchg_i64(&m, b);
                uint64_t want = (uint64_t)(b);
                if (old != a || m != want) {
                    if (failures++ < 20) printf("xchg.i64(%#llx, %#llx): old %#llx new %#llx\n",
                        (unsigned long long)a, (unsigned long long)b, (unsigned long long)old, (unsigned long long)m);
                }
            }
            {
                uint64_t a = (uint64_t)values[i], b = (uint64_t)values[j], m = a;
                uint64_t old = k_umin_i64(&m, b);
                uint64_t want = (uint64_t)(a < b ? a : b);
                if (old != a || m != want) {
                    if (failures++ < 20) printf("umin.i64(%#llx, %#llx): old %#llx new %#llx\n",
                        (unsigned long long)a, (unsigned long long)b, (unsigned long long)old, (unsigned long long)m);
                }
            }
            {
                uint64_t a = (uint64_t)values[i], b = (uint64_t)values[j], m = a;
                uint64_t old = k_umax_i64(&m, b);
                uint64_t want = (uint64_t)(a > b ? a : b);
                if (old != a || m != want) {
                    if (failures++ < 20) printf("umax.i64(%#llx, %#llx): old %#llx new %#llx\n",
                        (unsigned long long)a, (unsigned long long)b, (unsigned long long)old, (unsigned long long)m);
                }
            }
            {
                int64_t a = (int64_t)values[i], b = (int64_t)values[j], m = a;
                int64_t old = k_smin_i64(&m, b);
                int64_t want = (int64_t)(a < b ? a : b);
                if (old != a || m != want) {
                    if (failures++ < 20) printf("smin.i64(%#llx, %#llx): old %#llx new %#llx\n",
                        (unsigned long long)a, (unsigned long long)b, (unsigned long long)old, (unsigned long long)m);
                }
            }
            {
                int64_t a = (int64_t)values[i], b = (int64_t)values[j], m = a;
                int64_t old = k_smax_i64(&m, b);
                int64_t want = (int64_t)(a > b ? a : b);
                if (old != a || m != want) {
                    if (failures++ < 20) printf("smax.i64(%#llx, %#llx): old %#llx new %#llx\n",
                        (unsigned long long)a, (unsigned long long)b, (unsigned long long)old, (unsigned long long)m);
                }
            }
            {
                uint64_t a = (uint64_t)values[i], b = (uint64_t)values[j], m = a;
                /* Succeeds: expected matches. */
                uint64_t old = k_cas_i64(&m, a, b);
                if (old != a || m != b) { failures++; printf("cas.i64 success wrong\n"); }
                /* Fails unless a == b: expected is a, memory holds b. */
                old = k_cas_i64(&m, a, (uint64_t)(a + 1));
                uint64_t want = a == b ? (uint64_t)(a + 1) : b;
                if (old != b || m != want) { failures++; printf("cas.i64 failure wrong\n"); }
                m = a;
                old = k_swap_i64(&m, b);
                if (old != a || m != b) { failures++; printf("swap.i64 wrong\n"); }
            }
        }
}

#define THREADS 4
#define ROUNDS 200000

static int64_t shared;
static int32_t lock;
static int64_t guarded;

static void *count(void *arg) {
    (void)arg;
    k_count(&shared, ROUNDS);
    return 0;
}

static void *locked(void *arg) {
    (void)arg;
    for (int i = 0; i < ROUNDS; i++) {
        k_lock(&lock);
        /* A plain read-modify-write, only safe under the lock. */
        guarded = *(volatile int64_t *)&guarded + 1;
        k_unlock(&lock);
    }
    return 0;
}

static void run(void *(*f)(void *)) {
    pthread_t t[THREADS];
    for (int i = 0; i < THREADS; i++) pthread_create(&t[i], 0, f, 0);
    for (int i = 0; i < THREADS; i++) pthread_join(t[i], 0);
}

int main(void) {
    check_i8();
    check_i16();
    check_i32();
    check_i64();
    int x, y;
    void *p = &x;
    if (k_xchg_ptr(&p, &y) != &y || p != &x) { failures++; printf("xchg ptr wrong\n"); }
    run(count);
    if (shared != (int64_t)THREADS * ROUNDS) { failures++; printf("counter %lld\n", (long long)shared); }
    run(locked);
    if (guarded != (int64_t)THREADS * ROUNDS) { failures++; printf("guarded %lld\n", (long long)guarded); }
    /* An exit status is taken mod 256. */
    return failures != 0;
}
