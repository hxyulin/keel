#include <pthread.h>
#include <stdint.h>
#include <stdio.h>
#include "clobber.h"

extern _Thread_local int64_t counter;
extern _Thread_local int32_t zeroed;
_Thread_local int64_t c_tls = 100;

int64_t k_bump(void);

#define THREADS 4

/* Each thread bumps its own copies `n` times; they start from the
   initial values in every thread. Returns the number of mismatches. */
static void *bump(void *arg) {
    int64_t id = (int64_t)arg, bad = 0;
    c_tls = 1000 * id;
    int n = 100 + (int)id;
    for (int i = 1; i <= n; i++) {
        int64_t got = k_bump();
        if (got != 5 + i + 2 * i + 1000 * id) bad++;
    }
    if (counter != 5 + n || zeroed != 2 * n) bad++;
    return (void *)bad;
}

int main(void) {
    pthread_t t[THREADS];
    int64_t bad = 0;
    for (int64_t i = 0; i < THREADS; i++) pthread_create(&t[i], 0, bump, (void *)(i + 1));
    for (int i = 0; i < THREADS; i++) {
        void *r;
        pthread_join(t[i], &r);
        bad += (int64_t)r;
    }
    /* The main thread's copies are untouched. */
    if (counter != 5 || zeroed != 0) bad++;
    if (k_bump() != 6 + 2 + 100) bad++;
    if (bad) printf("%lld mismatches\n", (long long)bad);
    /* An exit status is taken mod 256. */
    return bad != 0;
}
