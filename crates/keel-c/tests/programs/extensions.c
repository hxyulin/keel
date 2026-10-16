/* What real programs and Apple's headers use beyond ISO C: `#pragma
 * pack`, `packed`, zero-length arrays, enums with a fixed type, GCC's
 * atomics and `_FORTIFY_SOURCE`'s checked builtins. Plus a declaration
 * whose initializer refers to the variable itself. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>

#pragma pack(push, 4)
struct pack4 { int a; double d; };
#pragma pack(pop)
struct natural { int a; double d; };
struct __attribute__((packed)) pack1 { char c; int i; };
struct __attribute__((aligned(16))) wide { int x; };
struct flex { int n; long items[0]; };
enum : uint8_t { SMALL = 200 } small_enum;

int main(void) {
    printf("%zu %zu %zu %zu %zu %zu\n", sizeof(struct pack4), sizeof(struct natural),
           sizeof(struct pack1), sizeof(struct wide), sizeof(struct flex), sizeof small_enum);
    struct pack1 p1 = { 'c', 0x01020304 };
    printf("%c %x\n", p1.c, p1.i);
    struct flex *f = malloc(sizeof *f + 3 * sizeof f->items[0]);
    f->n = 3;
    for (int i = 0; i < f->n; i++) f->items[i] = i * 100;
    printf("%d %ld\n", f->n, f->items[2]);
    free(f);
    int x = 5;
    __atomic_store_n(&x, 7, __ATOMIC_RELEASE);
    printf("%d\n", __atomic_load_n(&x, __ATOMIC_ACQUIRE) + __atomic_load_n(&x, __ATOMIC_SEQ_CST));
    char buf[8];
    memset(buf, 'z', sizeof buf); /* __builtin___memset_chk */
    memcpy(buf, "abc", 4);        /* __builtin___memcpy_chk */
    printf("%s %c\n", buf, buf[7]);
    return 0;
}
