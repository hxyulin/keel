#include <unwind.h>
#include <stdio.h>
static int frames;
static _Unwind_Reason_Code count(struct _Unwind_Context *context, void *arg) {
    (void)context; (void)arg; ++frames; return _URC_NO_REASON;
}
static int middle(int x) {
    _Unwind_Backtrace(count, 0);
    return x + frames;
}
int main(void) {
    int n = middle(7);
    printf("backtrace: %d frames\n", frames);
    return n < 9;
}
