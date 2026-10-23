/* Unsigned byte ordering yields canonical all-ones/zero byte masks. */
typedef unsigned char U __attribute__((vector_size(16)));
void kernel(void *dp, const void *ap, const void *bp) {
    U a = *(const U*)ap, b = *(const U*)bp;
    *(U*)dp = (U)(a < b);
}
