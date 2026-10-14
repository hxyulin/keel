/* Built with `-g` by the `debugger` test, which inspects it in lldb. */
struct point { int x, y; };
struct node { int value; struct node *next; };
struct flags { unsigned a : 3, b : 5; int c; };
struct wrap { int tag; union { long l; double d; }; };

static int counter = 5;
double scale = 1.5;

void stop(void) {}

int area(struct point p, int k) {
    int r = p.x * p.y * k;
    return r;
}

int sum(int n) {
    int total = 0;
    for (int i = 0; i < n; i++)
        total += i;
    return total;
}

int main(void) {
    struct point p = { 3, 4 };
    struct node b = { 2, 0 }, a = { 1, &b };
    struct flags fl = { 5, 17, -3 };
    struct wrap w = { 7, { .d = 0.25 } };
    int arr[4] = { 1, 2, 3, 4 };
    char name[8] = "keel";
    double d = 3.25;
    int *ptr = &arr[2];
    stop();
    int s = sum(p.x + p.y);
    return s + area(p, 2) + (int)d + counter;
}
