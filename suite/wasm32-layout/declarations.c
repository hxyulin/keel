/* Declaration-only Clang oracle; no headers, object generation, or Wasm runtime.
 * Run commands and acceptance boundaries: docs/wasm32-layout-plan.md. */
#define ASSERT(c) _Static_assert((c), #c)
#define SAME(a, b) __builtin_types_compatible_p(a, b)
#define TYPE(e, t) SAME(__typeof__(e), t)
#define OFF(t, m) __builtin_offsetof(t, m)

ASSERT(sizeof(void *) == 4);
ASSERT(_Alignof(void *) == 4);
ASSERT(sizeof(long) == 4);
ASSERT(sizeof(long long) == 8);
ASSERT(_Alignof(long long) == 8);
ASSERT(sizeof(long double) == 16);
ASSERT(_Alignof(long double) == 16);
ASSERT(sizeof(_Complex long double) == 32);
ASSERT(_Alignof(_Complex long double) == 16);
ASSERT((char)-1 < 0);
ASSERT(SAME(__SIZE_TYPE__, unsigned long));
ASSERT(SAME(__PTRDIFF_TYPE__, long));
ASSERT(SAME(__INTPTR_TYPE__, long));
ASSERT(SAME(__UINTPTR_TYPE__, unsigned long));
ASSERT(SAME(__INT64_TYPE__, long long));
ASSERT(SAME(__INTMAX_TYPE__, long long));

ASSERT(TYPE(2147483648, long long));
ASSERT(TYPE(2147483648L, long long));
ASSERT(TYPE(0x80000000, unsigned int));
ASSERT(TYPE(0x80000000L, unsigned long));
ASSERT(TYPE(4294967296UL, unsigned long long));
ASSERT(TYPE(1L + 1U, unsigned long));
ASSERT(TYPE(1LL + 1UL, long long));
ASSERT(TYPE(+(unsigned short)1, int));
ASSERT(TYPE(sizeof(int), unsigned long));
ASSERT(TYPE((char *)0 - (char *)0, long));
ASSERT((unsigned long)-1 == 4294967295UL);
ASSERT((unsigned long)4294967296ULL == 0UL);

typedef signed int SI __attribute__((mode(SI)));
typedef signed int DI __attribute__((mode(DI)));
typedef unsigned int UDI __attribute__((mode(DI)));
typedef signed int WORD __attribute__((mode(word)));
typedef signed int POINTER __attribute__((mode(pointer)));
ASSERT(SAME(SI, int));
ASSERT(SAME(DI, long long));
ASSERT(SAME(UDI, unsigned long long));
ASSERT(SAME(WORD, int));
ASSERT(SAME(POINTER, int));

struct Pair { char tag; void *p; long n; };
union Payload { void *p; long long n; char bytes[9]; };
struct Nested { char tag; struct Pair pair[2]; union Payload payload; char end; };
struct Extended { char tag; long double value; void *p; };
ASSERT(sizeof(struct Pair) == 12);
ASSERT(_Alignof(struct Pair) == 4);
ASSERT(OFF(struct Pair, p) == 4);
ASSERT(OFF(struct Pair, n) == 8);
ASSERT(sizeof(union Payload) == 16);
ASSERT(_Alignof(union Payload) == 8);
ASSERT(OFF(struct Nested, pair) == 4);
ASSERT(OFF(struct Nested, payload) == 32);
ASSERT(OFF(struct Nested, end) == 48);
ASSERT(sizeof(struct Nested) == 56);
ASSERT(_Alignof(struct Nested) == 8);
ASSERT(OFF(struct Extended, value) == 16);
ASSERT(OFF(struct Extended, p) == 32);
ASSERT(sizeof(struct Extended) == 48);
ASSERT(_Alignof(struct Extended) == 16);

/* Type/expression alignment does not establish actual object placement. */
char large_array[17];
struct ArrayMember { char tag; char bytes[17]; };
ASSERT(_Alignof(char[17]) == 1);
ASSERT(__alignof__(large_array) == 1);
ASSERT(OFF(struct ArrayMember, bytes) == 1);
ASSERT(sizeof(struct ArrayMember) == 18);

/* Declaration safety only: Keel must reject evaluated extended FP and its ABI. */
extern long double extended(long double);
typedef long double (*ExtendedFunction)(long double);
extern struct Extended *extended_object;

/* These need later byte/fixup tests; syntax success does not prove relocation emission. */
char data[12];
struct PointerInit { char tag; char *p; long n; } pointer_init = { 1, data + 3, 7 };
void *null_pointer = (void *)0;
void *high_pointer = (void *)0x80000000UL;
