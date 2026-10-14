/* Control flow: loops, switch with fallthrough and a default in the
 * middle, goto, short-circuit evaluation, and setjmp/longjmp. */
#include <stdio.h>
#include <setjmp.h>

static int calls;
static int side(int v) { calls++; return v; }

static int classify(int n) {
    int r = 0;
    switch (n) {
    case 1:
        r += 1;
    case 2:
        r += 10;
        break;
    default:
        r = -1;
    case 5:
        r += 100;
        break;
    case 1000000:
        r = 7;
    }
    return r;
}

static int collatz(long n) {
    int steps = 0;
again:
    if (n == 1) goto done;
    n = n % 2 ? 3 * n + 1 : n / 2;
    steps++;
    goto again;
done:
    return steps;
}

static jmp_buf env;
static void deep(int n) {
    if (n == 0) longjmp(env, 42);
    deep(n - 1);
}

int main(void) {
    for (int i = 0; i < 7; i++) printf("%d ", classify(i));
    printf("%d\n", classify(1000000));
    int sum = 0;
    for (int i = 0; i < 10; i++) {
        if (i == 2) continue;
        for (int j = 0; j < 10; j++) {
            if (j > i) break;
            sum += j;
        }
        if (i == 8) break;
    }
    int k = 0;
    while (k < 100) k += 7;
    do k--; while (k > 95);
    printf("%d %d %d\n", sum, k, collatz(27));
    calls = 0;
    int r = side(0) && side(1);
    r += side(1) || side(1);
    r += (side(0) || side(0)) + !side(3);
    printf("%d %d\n", r, calls);
    int a = 3, b = 4;
    int c = (a++, b++, a + b);
    printf("%d %d\n", c, a > b ? a : b);
    volatile int saved = 1;
    int status = setjmp(env);
    if (status == 0) {
        saved = 2;
        deep(10);
        printf("not reached\n");
    }
    printf("%d %d\n", status, saved);
    /* A switch whose cases sit inside a loop: Duff's device. */
    char out[20] = { 0 };
    const char *src = "duffs device!";
    char *to = out;
    int count = 13;
    int n = (count + 7) / 8;
    switch (count % 8) {
    case 0: do { *to++ = *src++;
    case 7:      *to++ = *src++;
    case 6:      *to++ = *src++;
    case 5:      *to++ = *src++;
    case 4:      *to++ = *src++;
    case 3:      *to++ = *src++;
    case 2:      *to++ = *src++;
    case 1:      *to++ = *src++;
            } while (--n > 0);
    }
    printf("%s\n", out);
    return 0;
}
