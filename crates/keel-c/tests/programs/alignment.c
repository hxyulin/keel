/* Member offsets, object alignment and packing must agree with Clang. */
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

struct member_aligned { char c; int x __attribute__((aligned(16))); char tail; };
struct member_packed { char c; int x __attribute__((packed)); char tail; };
struct __attribute__((packed)) packed_aligned {
    char c; int x __attribute__((aligned(16))); char tail;
};
struct alignas_member { char c; _Alignas(16) int x; char tail; };
struct prefix_member { char c; __attribute__((aligned(16))) int x; };
struct declarator_member { char c; int __attribute__((aligned(16))) x; };
struct pointer_member { char c; int * __attribute__((aligned(16))) x; };
union member_union { char c; long x __attribute__((aligned(32))); };
struct nested { char c; struct member_aligned x; char tail; };

#pragma pack(push, 2)
struct pragma_aligned { char c; int x __attribute__((aligned(16))); char tail; };
#pragma pack(pop)

struct __attribute__((aligned(64))) high { long x; char tail; };
struct __attribute__((packed)) packed_bits { char c; unsigned a:3; unsigned b:8; char tail; };

_Alignas(32) int global = 5;
static long array[3] __attribute__((aligned(64)));
extern int redeclared __attribute__((aligned(64)));
int redeclared;
_Alignas(32) _Thread_local int thread_value = 9;

#define STRUCT_LAYOUT(T, F) \
    printf(#T " %zu %zu %zu %zu\n", sizeof(struct T), _Alignof(struct T), \
           offsetof(struct T, F), __alignof__(((struct T *)0)->F))

static void bump(int *p) { (*p)++; }

int main(void) {
    STRUCT_LAYOUT(member_aligned, x);
    STRUCT_LAYOUT(member_packed, x);
    STRUCT_LAYOUT(packed_aligned, x);
    STRUCT_LAYOUT(alignas_member, x);
    STRUCT_LAYOUT(prefix_member, x);
    STRUCT_LAYOUT(declarator_member, x);
    STRUCT_LAYOUT(pointer_member, x);
    STRUCT_LAYOUT(pragma_aligned, x);
    STRUCT_LAYOUT(nested, x);
    printf("union %zu %zu %zu\n", sizeof(union member_union), _Alignof(union member_union),
           __alignof__(((union member_union *)0)->x));
    printf("global %zu %zu %zu %zu %zu %zu\n", __alignof__(global), (uintptr_t)&global % 32,
           __alignof__(array), (uintptr_t)array % 64, __alignof__(redeclared), (uintptr_t)&redeclared % 64);
    printf("thread %zu %zu %d\n", __alignof__(thread_value), (uintptr_t)&thread_value % 32, thread_value);

    _Alignas(32) int local = 10;
    long local_array[3] __attribute__((aligned(64))) = { 1, 2, 3 };
    _Alignas(struct high) long by_type = 7;
    _Alignas(0) int natural = 8;
    static int saved __attribute__((aligned(32))) = 11;
    int __attribute__((aligned(16))) prefix = 12;
    int * __attribute__((aligned(32))) pointer = &local;
    struct high objects[2] = { { 13, 'a' }, { 14, 'b' } };
    union member_union u = { .x = 15 };
    struct high *literal = &(struct high){ 16, 'c' };
    char *dynamic = __builtin_alloca(13);
    dynamic[0] = 3;
    bump(&local);
    printf("locals %zu %zu %zu %zu %zu %zu %zu %zu\n", __alignof__(local), (uintptr_t)&local % 32,
           __alignof__(local_array), (uintptr_t)local_array % 64, __alignof__(by_type), (uintptr_t)&by_type % 64,
           __alignof__(pointer), (uintptr_t)&pointer % 32);
    printf("more %zu %zu %zu %zu %zu %zu %zu %zu\n", __alignof__(natural), __alignof__(prefix),
           (uintptr_t)&prefix % 16, __alignof__(saved), (uintptr_t)&saved % 32,
           (uintptr_t)objects % 64, (uintptr_t)&objects[1] % 64, (uintptr_t)&u % 32);
    printf("values %d %ld %ld %d %ld %ld %zu %d\n", local, local_array[2], by_type, saved,
           objects[1].x, literal->x, (uintptr_t)literal % 64, dynamic[0]);

    struct member_packed p = { 'p', 12345, 't' };
    struct member_aligned a = { 'a', 23456, 'z' };
    struct packed_bits bits = { 'b', 5, 201, 'q' };
    printf("members %c %d %c %c %d %c\n", p.c, p.x, p.tail, a.c, a.x, a.tail);
    printf("bits %zu %zu %c %u %u %c\n", sizeof bits, _Alignof(struct packed_bits),
           bits.c, bits.a, bits.b, bits.tail);
    return 0;
}
