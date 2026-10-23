/* Supported GNU vector builtin; aligned pointer-only ABI. */
typedef unsigned U __attribute__((vector_size(16)));
void kernel(void *dp, const void *ap, const void *bp) {
    U a = *(const U*)ap;
    (void)bp;
    *(U*)dp = __builtin_shufflevector(a, a, 2, 0, 3, 1);
}
