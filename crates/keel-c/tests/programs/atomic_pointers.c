/* Atomic pointer increments: result values, scaling and single evaluation. */
#include <stddef.h>
#include <stdio.h>

static int values[8];
static _Atomic(int *) global;
static int calls;

static _Atomic(int *) *next_pointer(void) {
    calls++;
    return &global;
}

struct item { long fields[3]; };
struct holder { _Atomic(struct item *) pointer; };

int main(void) {
    _Atomic(int *) p = values + 3;
    int *result = p++;
    printf("post++ %td %td\n", result - values, p - values);
    result = ++p;
    printf("pre++ %td %td\n", result - values, p - values);
    result = p--;
    printf("post-- %td %td\n", result - values, p - values);
    result = --p;
    printf("pre-- %td %td\n", result - values, p - values);

    global = values + 3;
    result = (*next_pointer())++;
    printf("call post++ %td %td %d\n", result - values, global - values, calls);
    result = ++*next_pointer();
    printf("call pre++ %td %td %d\n", result - values, global - values, calls);
    result = (*next_pointer())--;
    printf("call post-- %td %td %d\n", result - values, global - values, calls);
    result = --*next_pointer();
    printf("call pre-- %td %td %d\n", result - values, global - values, calls);

    char bytes[8];
    _Atomic(char *) byte = bytes + 3;
    char *old_byte = byte++;
    printf("byte %td %td\n", old_byte - bytes, byte - bytes);

    struct item items[8];
    struct holder h = { items + 3 };
    struct item *old_item = h.pointer++;
    printf("struct %td %td\n", old_item - items, h.pointer - items);
    struct item *new_item = --h.pointer;
    printf("struct pre-- %td %td\n", new_item - items, h.pointer - items);

    int arrays[8][3];
    _Atomic(int (*)[3]) array = arrays + 3;
    int (*old_array)[3] = array++;
    printf("array %td %td\n", old_array - arrays, array - arrays);
    return 0;
}
