/* Header-free so the explicit builtins cannot alias libc declarations. */
int printf(const char *, ...);
static unsigned char dst[8192], src[8192], expected[8192];
static int dst_calls, src_calls, len_calls, fill_calls, bound_calls;
static unsigned long length;
static void *destination(void) { ++dst_calls; return dst; }
static const void *source(void) { ++src_calls; return src; }
static unsigned long count(void) { ++len_calls; return length; }
static int fill(void) { ++fill_calls; return 0x12345; }
static unsigned long bound(void) { ++bound_calls; return 8192; }
void *__memcpy_chk(void *, const void *, unsigned long, unsigned long);
static void reset(void) {
    for (int i = 0; i < 8192; ++i) {
        dst[i] = expected[i] = (unsigned char)(i * 17 + 9);
        src[i] = (unsigned char)(i * 31 + 7);
    }
    dst_calls = src_calls = len_calls = fill_calls = 0;
}
static int compare(void) {
    int errors = 0;
    for (int i = 0; i < 8192; ++i) errors += dst[i] != expected[i];
    return errors;
}
int main(void) {
    unsigned long sizes[] = {0, 1, 7, 32, 257, 4096};
    int errors = 0;
    unsigned long hash = 0;
    for (int k = 0; k < 6; ++k) {
        length = sizes[k];
        reset();
        errors += __builtin_memcpy(destination(), source(), count()) != dst;
        errors += dst_calls != 1 || src_calls != 1 || len_calls != 1;
        for (unsigned long i = 0; i < length; ++i) expected[i] = src[i];
        errors += compare();
        reset();
        errors += __builtin_memset(destination(), fill(), count()) != dst;
        errors += dst_calls != 1 || fill_calls != 1 || len_calls != 1;
        for (unsigned long i = 0; i < length; ++i) expected[i] = 0x45;
        errors += compare();
        reset();
        errors += __builtin_memmove(dst + 3, dst, length) != dst + 3;
        for (unsigned long i = length; i > 0; --i) expected[i + 2] = expected[i - 1];
        errors += compare();
        reset();
        errors += __builtin_memmove(dst, dst + 3, length) != dst;
        for (unsigned long i = 0; i < length; ++i) expected[i] = expected[i + 3];
        errors += compare();
        errors += __builtin_memmove(destination(), destination(), count()) != dst;
        errors += dst_calls != 2 || len_calls != 1;
        for (int i = 0; i < 8192; ++i) hash = hash * 33 + dst[i];
    }
    reset();
    errors += __builtin_memcpy(destination(), source(), 0) != dst;
    errors += dst_calls != 1 || src_calls != 1;
    errors += compare();
    errors += __builtin_memcpy(dst, src, 7) != dst;
    for (int i = 0; i < 7; ++i) expected[i] = src[i];
    errors += compare();
    errors += __builtin_memcpy(dst, src, 4096) != dst;
    for (int i = 0; i < 4096; ++i) expected[i] = src[i];
    errors += compare();
    errors += __builtin_memset(dst, -257, (unsigned short)7) != dst;
    for (int i = 0; i < 7; ++i) expected[i] = 255;
    errors += compare();
    errors += __builtin_memset(dst, 257.0, 7.9) != dst;
    for (int i = 0; i < 7; ++i) expected[i] = 1;
    errors += compare();
    bound_calls = 0;
    errors += __builtin___memcpy_chk(dst, src, 7, bound()) != dst;
    errors += bound_calls != 1;
    printf("%d %lu\n", errors, hash);
    return errors != 0;
}
