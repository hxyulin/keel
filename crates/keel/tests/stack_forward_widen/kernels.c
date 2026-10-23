/* Pointer ABI. The C vector dereferences require 16-byte alignment. */
typedef int S __attribute__((vector_size(16)));
typedef long L __attribute__((vector_size(16)));
extern void *once(void *);
extern void clobber(void *);
void PREFIXplain(void *out, void *input) {
    S a = *(const S *)input;
    *(L *)out = (L){(long)a[0], (long)a[1]};
}
void PREFIXsnapshot(void *out, void *input) {
    S a = *(const S *)once(input);
    clobber(input);
    *(L *)out = (L){(long)a[0], (long)a[1]};
}
