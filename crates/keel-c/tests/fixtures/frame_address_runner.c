#include <stdio.h>

extern void *keel_frame_0(void), *keel_frame_1(void), *keel_frame_2(void), *keel_frame_3(void);
extern void *keel_frame_dynamic(unsigned);

int check_frame_buffer(const char *buffer, unsigned n) {
    return n < 2 || buffer[0] != 5 || buffer[n - 1] != 9;
}

/* Compile every caller with frame pointers and sibling calls disabled. The
   builtins cannot reconstruct records omitted by a caller or read past them. */
__attribute__((noinline)) static int child(void *parent_frame, void *grandparent_frame) {
    void *here = __builtin_frame_address(0);
    if (keel_frame_1() != here) return 1;
    if (keel_frame_2() != parent_frame) return 2;
    if (keel_frame_3() != grandparent_frame) return 3;
    void *callee_frame = keel_frame_0();
    if (!callee_frame || callee_frame == here) return 4;
    for (unsigned n = 2; n < 260; n += 17) {
        if (keel_frame_dynamic(n) != here) return 5;
    }
    return 0;
}
__attribute__((noinline)) static int parent(void *main_frame) {
    return child(__builtin_frame_address(0), main_frame);
}
int main(void) {
    int result = parent(__builtin_frame_address(0));
    if (result) fprintf(stderr, "frame-address check failed: %d\n", result);
    else puts("frame-address PASS");
    return result;
}
