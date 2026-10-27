/* Caller records must be readable and use the native saved-FP ABI. */
__attribute__((noinline)) void *keel_frame_0(void) {
    return __builtin_frame_address(0);
}
__attribute__((noinline)) void *keel_frame_1(void) {
    return __builtin_frame_address(1);
}
__attribute__((noinline)) void *keel_frame_2(void) {
    return __builtin_frame_address(2);
}
__attribute__((noinline)) void *keel_frame_3(void) {
    return __builtin_frame_address(3);
}
extern int check_frame_buffer(const char *, unsigned);
__attribute__((noinline)) void *keel_frame_dynamic(unsigned n) {
    char buffer[n];
    buffer[0] = 5;
    buffer[n - 1] = 9;
    if (check_frame_buffer(buffer, n)) return (void *)0;
    return __builtin_frame_address(1);
}
