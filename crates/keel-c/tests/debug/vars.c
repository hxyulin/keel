/* Built with `-g` by the `debugger` test, which inspects it in lldb. */
typedef unsigned long size_t;
struct point { int x, y; };
typedef struct point point_t;
struct node { int value; struct node *next; };
struct flags { unsigned a : 3, b : 5; int c; };
struct wrap { int tag; union { long l; double d; }; };
enum color { RED, GREEN = 5, BLUE };

static int counter = 5;
double scale = 1.5;
_Thread_local int per_thread = 8;

void stop(void) {}

int area(point_t p, int k) {
    int r = p.x * p.y * k;
    return r;
}

int sum(int n) {
    int total = 0;
    for (int i = 0; i < n; i++) {
        int sq = i * i;
        total += i;
        if (sq == 4)
            stop();
    }
    {
        int total = 99;
        stop();
    }
    return total;
}

int main(void) {
    point_t p = { 3, 4 };
    struct node b = { 2, 0 }, a = { 1, &b };
    struct flags fl = { 5, 17, -3 };
    struct wrap w = { 7, { .d = 0.25 } };
    int arr[4] = { 1, 2, 3, 4 };
    char name[8] = "keel";
    double d = 3.25;
    int *ptr = &arr[2];
    const char *greeting = name;
    enum color c = GREEN;
    size_t len = sizeof arr;
    volatile int v = 1;
    int s = 0;
    stop();
    s = sum(p.x + p.y);
    return s + area(p, 2) + (int)d + counter + (c == GREEN) + (int)len + v + per_thread
        - (greeting[0] != 'k');
}
