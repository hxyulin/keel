#include <stdio.h>
#include <stdint.h>

typedef int Wide __attribute__((aligned(16)));
typedef Wide Alias;
typedef long Small __attribute__((aligned(1)));
typedef struct { long a; char b; } Record __attribute__((aligned(32)));
typedef struct { long a[8]; void *pad[4]; } PthreadBuf __attribute__((aligned));
struct Members { char c; Wide w; Small s; Record r; };
struct __attribute__((packed)) Packed { char c; Wide w; };
_Static_assert(sizeof(Wide) == sizeof(int), "alias size");
_Static_assert(_Alignof(Alias) == 16, "inherited alignment");
_Static_assert(_Alignof(Small) == 1, "reduced alignment");
_Static_assert(sizeof(Record) == 16 && _Alignof(Record) == 32, "record alias");
_Static_assert(sizeof(PthreadBuf) == 96 && _Alignof(PthreadBuf) == 16, "pthread layout");
_Static_assert(__builtin_offsetof(struct Members, w) == 16, "member");
_Static_assert(__builtin_offsetof(struct Members, s) == 20, "reduced member");
_Static_assert(__builtin_offsetof(struct Members, r) == 32, "record member");
_Static_assert(sizeof(struct Members) == 64, "member layout");
_Static_assert(sizeof(struct Packed) == 5, "packed overrides alias");
static Wide global;
static Small small;
_Static_assert(_Alignof(small) == 1, "global reduced alignment");
static Record record;
int main(void) {
    Wide local = 3;
    Small values[2] = {4, 5};
    PthreadBuf buf[2];
    Alias *p = &local;
    Record *q = &record;
    _Static_assert(_Alignof(*p) == 16, "pointee");
    _Static_assert(_Alignof(*q) == 32, "record pointee");
    _Static_assert(_Alignof(values) == 1, "array alias");
    _Static_assert(__builtin_types_compatible_p(Wide, int), "compatibility");
    _Alignas(Wide) char storage;
    int ok = !((uintptr_t)&global % 16) && !((uintptr_t)&local % 16)
        && !((uintptr_t)&record % 32) && !((uintptr_t)&buf % 16)
        && !((uintptr_t)&storage % 16) && !((uintptr_t)&(Record){0} % 32);
    printf("%d %d %ld\n", ok, local, values[0] + values[1]);
    return !ok;
}
