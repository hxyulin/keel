extern int printf(const char *, ...);
typedef int Count;
int effects;
int tick(void) { ++effects; return 2; }
// All these expressions are type-checked but never executed.
long matrix(Count rows, int cols, int a[static tick()][cols++]);
long matrix(int, int, int [*][*]);
long matrix(Count rows, int cols, int a[restrict static rows][cols++]) {
    unsigned long size = sizeof *a;
    int last = a[rows-1][cols-2];
    cols = 1;
    return last + size + sizeof *a + (a+rows-a);
}
long cube(int n, int m, int k, int (*a)[m][k]) {
    unsigned long plane = sizeof *a, row = sizeof **a;
    n=1; m=1; k=1;
    return a[1][2][3] + plane + row + sizeof *a + sizeof **a;
}
long ordered(int n, int a[n++], int b[n++], int (*c)[n++]) {
    unsigned long size = sizeof *c;
    n=1;
    return size + sizeof *c + sizeof a + sizeof b + a[0] + b[0];
}
long interleaved(int n, int a[n++], int m, int b[m++][n]) {
    unsigned long stride=sizeof *b;
    n=1; m=1;
    return a[0] + b[1][2] + stride + sizeof *b;
}
long outer(int n, int a[tick()]) { return n + effects + sizeof a; }
long qualified(int n, const int a[const volatile restrict static n]) {
    return a[n-1] + sizeof a;
}
long typedef_use(int n, int a[n][3]) {
    typedef int Row[n];
    long helper(Row *);
    long (*fp)(Row *) = 0;
    return sizeof *a + sizeof fp;
}
long alias_prototype(int n, int m, int a[n][m]) {
    typedef int Row[m++];
    long local(int, int, Row *);
    long not_evaluated(int, int, int [n++][m++]);
    long (*fp)(int, int, Row *)=matrix;
    return fp(n,3,a) + sizeof(Row) + n + m;
}
long flat(int width, int a[width]) { return a[width-1]; }
long nested_cb(int n, long (*cb)(int width, int a[width]), int a[n]) {
    return cb(n,a);
}
long dependent_size(int n, int (*p)[n++], int a[sizeof *p]) {
    n=1;
    return sizeof *p + a[0];
}
int n = 7; // no parameter name escapes a prototype or a definition
int prototype_scope(int n, int a[n]);
typedef int N;
int shadows(int N, int a[N]);
int shadows(int N, int a[N]) { return a[N-1]; }
long (*callback)(int n, int m, int a[n][m]) = matrix;
int tag_param(struct Parameter { int value; } p) {
    struct Parameter q = p;
    return q.value;
}
int main(void) {
    int a[2][3]={{1,2,3},{4,5,6}};
    int b[2][3][4]; b[1][2][3]=17;
    int x[4]={9,8,7,6};
    int c[2][4];
    long direct=matrix(2,3,a), indirect=callback(2,3,a);
    long o=outer(3,x);
    printf("%ld %ld %ld %ld\n", alias_prototype(2,3,a), nested_cb(4,flat,x), dependent_size(3,a,x), interleaved(2,x,2,a));
    printf("%ld %ld %ld %ld %ld %ld %ld %d %d %d\n", direct, indirect,
        cube(2,3,4,b), ordered(2,x,x,c), o, qualified(4,x),
        typedef_use(2,a), effects, shadows(4,x), n);
    return 0;
}
