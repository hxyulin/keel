#include <stdio.h>

void *read_return(void);
void *read_inlined_return(void);
void *read_return_after_call(void);
void *read_aligned_return(void);
void *read_dynamic_return(void);
void *return_identity_once(void *);
extern int return_address_evaluations;
int observe_return(void *(*)(void));
int observe_return_chain(void);
static int bad_buffer;
void touch_return_buffer(void *buffer) {
    if (*(unsigned char *)buffer != 29) bad_buffer = 1;
}

int main(void) {
    void *(*functions[])(void) = {
        read_return, read_inlined_return, read_return_after_call,
        read_aligned_return, read_dynamic_return
    };
    for (unsigned i = 0; i < sizeof(functions) / sizeof(functions[0]); ++i) {
        if (observe_return(functions[i])) {
            fprintf(stderr, "return address witness %u disagreed with actual CALL continuation\n", i);
            return 1;
        }
    }
    if (bad_buffer || return_identity_once(&bad_buffer) != &bad_buffer
        || return_address_evaluations != 1) return 2;
    if (observe_return_chain()) return 3;
    puts("return-address PASS");
    return 0;
}
