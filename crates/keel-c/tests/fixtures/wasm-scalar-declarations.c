/* Shared private-parser / Clang declaration oracle. No preprocessing, headers,
 * IR generation, object emission or Wasm C runtime is needed.
 * clang --target=wasm32-unknown-unknown -std=gnu11 -ffreestanding -nostdinc
 *   -Werror -fsyntax-only wasm-scalar-declarations.c
 * Also valid as an LP64 native control. */
typedef signed int DI __attribute__((mode(DI)));
typedef unsigned short UDI __attribute__((__mode__(__DI__)));
typedef signed long Word __attribute__((mode(word)));
typedef unsigned long Pointer __attribute__((__mode__(__pointer__)));
typedef int SI __attribute__((mode(__SI__)));
typedef unsigned int Byte __attribute__((__mode__(__byte__)));
_Static_assert(sizeof(DI) == 8, "DI remains 64 bits");
_Static_assert(__builtin_types_compatible_p(DI, long long) == (sizeof(long) == 4), "DI identity");
_Static_assert(__builtin_types_compatible_p(DI, long) == (sizeof(long) == 8), "native DI identity");
_Static_assert(__builtin_types_compatible_p(UDI, unsigned long long) == (sizeof(long) == 4), "unsigned DI identity");
_Static_assert(__builtin_types_compatible_p(Word, int) == (sizeof(long) == 4), "word identity");
_Static_assert(__builtin_types_compatible_p(Pointer, unsigned int) == (sizeof(long) == 4), "pointer identity");
_Static_assert(sizeof(Word) == sizeof(void *), "word width");
_Static_assert(sizeof(Pointer) == sizeof(void *), "pointer width");
_Static_assert(__builtin_types_compatible_p(SI, int), "SI identity");
_Static_assert(__builtin_types_compatible_p(Byte, unsigned char), "byte identity");
_Static_assert(__builtin_types_compatible_p(__typeof__(2147483648), long long) == (sizeof(long) == 4), "literal identity");
_Static_assert(__builtin_types_compatible_p(__typeof__(1U + -2L), unsigned long) == (sizeof(long) == 4), "conversion identity");
struct Inner { char tag; long value; void *ptr; };
union Payload { struct Inner inner; long long wide; void *ptrs[3]; };
struct Outer { char head; union Payload payload[2]; struct Inner tail; };
_Static_assert(sizeof(struct Inner) == 3 * sizeof(void *), "inner size");
_Static_assert(_Alignof(struct Inner) == sizeof(void *), "inner alignment");
_Static_assert(__builtin_offsetof(struct Inner, ptr) == 2 * sizeof(void *), "pointer offset");
_Static_assert(sizeof(union Payload) == (sizeof(long) == 4 ? 16 : 24), "union padding");
_Static_assert(_Alignof(union Payload) == 8, "union alignment");
_Static_assert(sizeof(struct Outer) == (sizeof(long) == 4 ? 56 : 80), "nested size");
_Static_assert(__builtin_offsetof(struct Outer, payload[1].ptrs[2]) == (sizeof(long) == 4 ? 32 : 48), "nested union array offset");
_Static_assert(__builtin_offsetof(struct Outer, tail.value) == (sizeof(long) == 4 ? 44 : 64), "nested field offset");
struct Inner grid[2][3];
long bounds[(unsigned long)0x100000001ULL == 1 ? 3 : 2];
_Static_assert(sizeof(bounds) == (sizeof(long) == 4 ? 12 : 16), "literal layout interaction");
_Static_assert(sizeof(grid) == 18 * sizeof(void *), "nested array stride");
struct Outer initialized = { .payload[1].ptrs[2] = (void *)0, .tail.value = (long)0x100000001L };
char narrow[] = "abc";
int wide[] = L"ab";
int clipped[2] = L"ab";
long scalar_values[3] = { [2] = (long)0x100000001L };

/* Recursive records, anonymous unions and aggregate array stride must use the
 * model at construction, including explicit record alignment and tail padding. */
struct Node {
    char head;
    union { long lanes[2]; void *links[2]; };
    struct Node *next;
} __attribute__((aligned(16)));
_Static_assert(sizeof(struct Node) == (sizeof(long) == 4 ? 16 : 32), "recursive size");
_Static_assert(_Alignof(struct Node) == 16, "explicit record alignment");
_Static_assert(__builtin_offsetof(struct Node, lanes[1]) == 2 * sizeof(long), "anonymous array offset");
_Static_assert(__builtin_offsetof(struct Node, next) == 3 * sizeof(void *), "recursive pointer offset");
struct Node nodes[] = { [1].lanes[1] = 7, [1].next = (void *)0 };
_Static_assert(sizeof(nodes) == 2 * sizeof(struct Node), "inferred record array");

struct Text { char marker; long pad; int wide[3]; char tail[4]; };
_Static_assert(sizeof(struct Text) == (sizeof(long) == 4 ? 24 : 32), "text record padding");
_Static_assert(__builtin_offsetof(struct Text, wide) == 2 * sizeof(long), "wide member placement");
struct Text texts[] = { [1].wide = { L"ab" }, [1].tail = "abc" };
_Static_assert(sizeof(texts) == 2 * sizeof(struct Text), "inferred string record array");
