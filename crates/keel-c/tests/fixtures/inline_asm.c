extern int printf(const char *, ...);
static int calls;
static long cells[4] = { 5, 9, 13, 17 };
static long *next_cell(void) { return &cells[calls++]; }
static long next_value(void) { ++calls; return 7; }
static long named(long a, long b) {
    long result;
    __asm__ __volatile__ __inline__("add %x[out], %x[left], %x[right]"
        : [out] "=&r"(result) : [left] "r"(a), [right] "r"(b) : "cc");
    return result;
}
static long tied(long x) {
    long result;
    __asm("add %0, %1, #%c2" : [dst] "=r"(result) : "[dst]"(x), "i"(3 * 7));
    return result;
}
static double floating(double x, double y) {
    __asm__("fadd %d0, %d0, %d1" : "+w"(x) : "w"(y));
    return x;
}
static float floating32(float x, float y) {
    float out;
    __asm__("fadd %s0, %s1, %s2" : "=w"(out) : "w"(x), "w"(y));
    return out;
}
static long load(long *p) {
    long result;
    __asm__("ldr %0, %1" : "=r"(result) : "m"(*p));
    return result;
}
static long clobbers(long a, long b, long c, long d, long e, long f,
                     long g, long h, long i, long j, long k, long l) {
    long before = a+b+c+d+e+f+g+h+i+j+k+l;
    __asm__ volatile("mov x9, #0\n\tmov x10, #0\n\tmov x19, #0\n\tfmov d8, xzr\n\tfmov d19, xzr"
        ::: "x9", "x10", "x19", "d8", "d19", "cc", "memory");
    return before + a*b + c*d + e*f + g*h + i*j + k*l;
}
static long effects(void) {
    calls = 0;
    __asm__("add %0, %0, #1" : "+r"(*next_cell()));
    __asm__("mov %0, #23" : "=r"(*next_cell()));
    long out;
    __asm__("add %0, %1, #2" : "=r"(out) : "r"(next_value()));
    long read;
    __asm__("ldr %0, %1" : "=r"(read) : "m"(*next_cell()));
    return cells[0]*10000 + cells[1]*100 + calls*10 + out + read;
}
static long barrier(void) {
    long x = 11;
    long *p = &x;
    __asm__ volatile("mov x9, #37\n\tstr x9, [%0]" :: "r"(p) : "x9", "memory");
    return x;
}
static long multi(long a, long b) {
    __asm__("add %0, %0, #2\n\tsub %1, %1, #3" : "+&r"(a), "+r"(b));
    return a*100+b;
}
static long numeric(long x) {
    long out;
    __asm__("add %0, %1, #%c2" : "=r"(out) : "0"(x), "n"(2));
    return out;
}
static int narrow(void) {
    signed char a;
    unsigned short b;
    __asm__("mov %w0, #255\n\tmov %w1, #65535" : "=r"(a), "=r"(b));
    return (a < 0) + (b >> 8) + a;
}
static long local_memory(void) {
    long x = 41, out;
    __asm__("ldr %0, %1" : "=r"(out) : "m"(x));
    return out;
}
static long indexed(void) {
    long array[2] = {10, 20};
    int i = 0;
    __asm__("add %0, %0, #3" : "+r"(array[i++]));
    return array[0] + i*100;
}
static double live_float(double a, double b) {
    double before = a*b;
    __asm__ volatile("fmov d8, xzr\n\tfmov d19, xzr" ::: "d8", "d19");
    return before + a + b;
}
static int carry(unsigned long a, unsigned long b) {
    unsigned long sum;
    unsigned flag;
    __asm__("adds %0, %2, %3\n\tcset %w1, cs" : "=&r"(sum), "=r"(flag) : "r"(a), "r"(b) : "cc");
    return flag + (sum == 1)*10;
}
// A named substitution ends at ']', even when a decimal digit follows.
static long named_digit_suffix(void) {
    long out;
    __asm__("mov %0, #%c[k]0" : "=r"(out)
        : [k] "i"(4), "i"(1), "i"(2), "i"(3), "i"(4),
          "i"(5), "i"(6), "i"(7), "i"(8), "i"(9));
    return out;
}
static long mixed_ties(long a, long b, long c) {
    long out;
    __asm__("add %1, %2, %3\n\tadd %0, %0, #1"
        : "+&r"(a), "=&r"(out) : "1"(b), "r"(c));
    return a*100 + out;
}
static long overlapping_readwrite(long x) {
    __asm__("add %0, %0, #1\n\tadd %1, %1, #1" : "+r"(x), "+r"(x));
    return x;
}
static long array_memory(int n) {
    long fixed[2] = {17, 19};
    long vm[n][2];
    vm[n-1][0] = 23;
    long a, b;
    __asm__("ldr %0, %2\n\tldr %1, %3" : "=&r"(a), "=r"(b)
        : "m"(fixed), "m"(vm[n-1]));
    return a*100+b;
}
static long volatile_output(void) {
    volatile long x = 10;
    __asm__("add %0, %0, #1" : "+r"(x));
    return x;
}
static long clobber_aliases(void) {
    long out;
    __asm__("mov w0, #9\n\tmov %0, #42\n\tmov x30, xzr"
        : "=r"(out) :: "w0", "lr", "v31", "d31");
    return out;
}
int main(void) {
    __asm__(""); __asm__ volatile("nop");
    long a = named(40, 2), b = tied(21), c = effects(), d = barrier();
    long e = clobbers(1,2,3,4,5,6,7,8,9,10,11,12), f = multi(5,8);
    double g = floating(1.25, 2.5);
    float h = floating32(2.5f, 4.0f);
    long m = load(&cells[2]);
    unsigned value = 5;
    __asm__("add %w0, %w0, #1" : "+r"(value));
    printf("%ld %ld %ld %ld %ld %ld %.2f %.2f %ld %u\n", a,b,c,d,e,f,g,(double)h,m,value);
    printf("%ld %d %ld %ld %.2f %d %d\n", numeric(40), narrow(), local_memory(), indexed(), live_float(1.5,2.0), carry(~0UL, 2), carry(3, 4));
    return !(mixed_ties(3,5,7)==412 && overlapping_readwrite(8)==9
        && array_memory(3)==1723 && volatile_output()==11
        && clobber_aliases()==42 && named_digit_suffix()==40
        && indexed()==113 && live_float(1.5,2.0)==6.5 && carry(~0UL,2)==11 && carry(3,4)==0 && numeric(40)==42 && narrow()==255 && local_memory()==41 && a==42 && b==42 && c==62366 && d==37 && e==400 && f==705 && g==3.75 && h==6.5f && m==13 && value==6);
}
