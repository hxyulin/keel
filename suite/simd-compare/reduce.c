/* Pointer ABI; aligned 16-byte inputs/output, no vector ABI assumptions. */
typedef unsigned U __attribute__((vector_size(16)));
typedef int S __attribute__((vector_size(16)));
typedef long L __attribute__((vector_size(16)));
typedef float F __attribute__((vector_size(16)));
void kernel(void *dp, const void *ap, const void *bp) {
    U *d = dp;
    U a = *(const U*)ap;
    unsigned s = a[0] + a[1] + a[2] + a[3]; *d = (U){s,s,s,s};
}
