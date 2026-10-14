/* Pointers: arithmetic, differences, function pointers and tables of
 * them, multi-dimensional arrays, and static data holding addresses. */
#include <stdio.h>
#include <string.h>
#include <stdlib.h>

static int add(int a, int b) { return a + b; }
static int sub(int a, int b) { return a - b; }
static int mul(int a, int b) { return a * b; }
static int (*const ops[])(int, int) = { add, sub, mul };
static const char *names[] = { "add", "sub", "mul" };
static int table[4][3] = { { 1, 2, 3 }, { 4, 5, 6 } };
static int *middle = &table[1][1];
static const char *tail = "hello world" + 6;
static struct { const char *name; int (*fn)(int, int); } named[] = {
    { "plus", add },
    { "minus", sub },
};

static int cmp(const void *a, const void *b) {
    return *(const int *)a - *(const int *)b;
}

static int apply(int (*f)(int, int), int x, int y) { return f(x, y); }

int main(void) {
    for (int i = 0; i < 3; i++) printf("%s=%d ", names[i], ops[i](7, 3));
    printf("%d %d\n", apply(mul, 6, 7), (*ops[1])(1, 2));
    printf("%d %d %s %s\n", *middle, middle[1], tail, named[1].name);
    printf("%d\n", named[0].fn(named[1].fn(10, 4), 1));
    int arr[10];
    for (int i = 0; i < 10; i++) arr[i] = (i * 7) % 10;
    qsort(arr, 10, sizeof arr[0], cmp);
    for (int *p = arr; p < arr + 10; p++) printf("%d", *p);
    int *end = &arr[9];
    printf(" %td %td %d\n", end - arr, arr - end, (int)(&arr[5] - &arr[2]));
    int (*rows)[3] = table;
    printf("%d %d %zu\n", rows[1][2], (*(rows + 1))[0], sizeof *rows);
    char buf[16];
    strcpy(buf, "abcdef");
    char *q = buf + 2;
    *q++ = 'X';
    q[1] = 'Y';
    printf("%s %c %d\n", buf, *--q, (int)(q - buf));
    void *v = buf;
    printf("%c\n", *((char *)v + 3));
    long addr = (long)(arr + 1) - (long)arr;
    printf("%ld %d\n", addr, arr + 3 == &arr[3]);
    struct node { int value; struct node *next; } n3 = { 3, 0 }, n2 = { 2, &n3 }, n1 = { 1, &n2 };
    int total = 0;
    for (struct node *n = &n1; n; n = n->next) total = total * 10 + n->value;
    printf("%d\n", total);
    return 0;
}
