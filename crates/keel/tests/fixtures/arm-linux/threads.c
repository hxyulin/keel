/* Independent libc/pthread harness; ABI callees are built by either compiler. */
#include <pthread.h>
#include "abi.h"
static void *worker(void *p) { (void)p; return (void *)(tls_next()==7 && tls_next()==8 ? 0L : 1L); }
int tls_thread_check(void) {
    pthread_t t; void *value=(void *)1;
    return pthread_create(&t,0,worker,0) || pthread_join(t,&value) || value;
}
