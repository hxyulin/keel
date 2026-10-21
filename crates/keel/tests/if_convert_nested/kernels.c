typedef unsigned long long U;
U positive(const int *a, U n) {
    U sum = 0;
    for (U i = 0; i < n; ++i) sum += a[i] > 0 ? (unsigned)a[i] : 0;
    return sum;
}
U clamped(const int *a, U n) {
    U sum = 0;
    for (U i = 0; i < n; ++i) {
        int x = a[i];
        sum += x < -128 ? 0 : x > 127 ? 255 : (U)(x + 128);
    }
    return sum;
}
U guarded(const int *a, U n) {
    U sum = 0;
    for (U i = 0; i < n; ++i) sum += a[i] == 0 ? 7 : (U)(128 / a[i]);
    return sum;
}
U volatile_read(const volatile int *a, U n) {
    U sum = 0;
    for (U i = 0; i < n; ++i) sum += a[i] > 0 ? (unsigned)a[i] : 0;
    return sum;
}
