#include <assert.h>
#include <stdint.h>
#include <stdlib.h>
#include <sys/wait.h>
#include <unistd.h>
struct input { const unsigned char *data; uint64_t length, position; };
extern unsigned char checked_peek_public(const struct input *, uint64_t);
void checked_panic(const void *record) {
    uint64_t code = *(const uint64_t *)record;
    _Exit(code == 1 ? 77 : 78);
}
int main(void) {
    unsigned char bytes[64];
    for (unsigned i=0; i<64; ++i) bytes[i]=(unsigned char)(i*37);
    for (unsigned pos=0; pos<70; ++pos) {
        struct input input={bytes,64,pos};
        for (unsigned delta=0; delta<70; ++delta)
            assert(checked_peek_public(&input,delta) == (pos+delta<64 ? bytes[pos+delta] : 0));
    }
    struct input empty={NULL,0,UINT64_MAX};
    assert(checked_peek_public(&empty,0)==0);
    pid_t child=fork();
    assert(child>=0);
    if (child==0) { checked_peek_public(&empty,1); _Exit(1); }
    int status=0;
    assert(waitpid(child,&status,0)==child);
    assert(WIFEXITED(status) && WEXITSTATUS(status)==77);
}
