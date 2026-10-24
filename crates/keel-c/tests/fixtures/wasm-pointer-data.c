/* Header-free private storage checkpoint. All call boundaries remain integers. */
#if defined(__wasm32__)
_Static_assert(sizeof(long) == 4 && sizeof(void *) == 4, "wasm32");
_Static_assert(_Alignof(void *) == 4, "pointer alignment");
#else
_Static_assert(sizeof(long) == 8 && sizeof(void *) == 8, "native control");
#endif
struct Storage { unsigned char before; long value; int *ptr; unsigned char after; };
static unsigned char tag[5] = {0xa5, 1, 2, 3, 0x5a};
static int numbers[3] = {11, 22, 33};
static int *links[3] = {numbers, numbers + 1, numbers + 3};
static unsigned long folded = (unsigned long)0x100000007ULL;
static void *numeric = (void *)0x80000001ULL;
static long helper(long x) { return x * 3 + 1; }
long recurse(long n) {
    volatile long here = n;
    volatile long *p = &here;
    if (n == 0) return *p;
    return recurse(n - 1) + *p;
}
unsigned long checksum(unsigned int address, int length) {
    unsigned char *input = (unsigned char *)(unsigned long)address;
    unsigned char local[16];
    struct Storage record;
    record.before = 0xa5;
    record.after = 0x5a;
    record.value = helper(length);
    record.ptr = links[1];
    unsigned long sum = folded;
    for (int i = 0; i < 16; i++) local[i] = (unsigned char)(i ^ 0x5a);
    for (int i = 0; i < length; i++) sum = (sum * 33) ^ input[i];
    for (int i = 0; i < 16; i++) sum ^= (unsigned long)local[i] << (i & 7);
    return sum + record.value + *record.ptr + record.before + record.after + tag[0] + tag[4];
}
long distance(void) { return links[2] - links[0]; }
unsigned long long pointer_bits(unsigned long long x) { return (unsigned long long)(void *)x; }
unsigned long long numeric_bits(void) { return (unsigned long long)numeric; }
long big_difference(unsigned int a, unsigned int b) { return (int *)(unsigned long)a - (int *)(unsigned long)b; }
int pointer_slot(void) { int *volatile p = numbers + 1; return *p; }
unsigned long long constant_pointer(void) { return (unsigned long long)(void *)0x1234567880000001ULL; }
int walk(unsigned int address, long offset) {
    unsigned char *p = (unsigned char *)(unsigned long)address;
    p += offset;
    p++;
    p -= 1;
    return *p;
}
int ordered(unsigned int a, unsigned int b) { return (void *)(unsigned long)a > (void *)(unsigned long)b; }
long stride(void) { struct T { int a, b, c; } x[3]; return &x[2] - &x[0]; }
int local_init(void) { int a[4] = {7, 0, 3, 4}; return a[0] + a[1] + a[2] + a[3]; }
