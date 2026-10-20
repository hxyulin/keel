#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <pthread.h>
extern int tls_count(void);
extern void *tls_addr(int);
extern int tls_alignment(int);
extern int tls_initial(int);
static pthread_mutex_t lock = PTHREAD_MUTEX_INITIALIZER;
static pthread_cond_t cond = PTHREAD_COND_INITIALIZER;
static int arrived;
static int failures;
static void check(int id) {
    int n=tls_count();
    if(n<1 || n>16) { fprintf(stderr,"ordinary TLS initialization/count failed\n"); abort(); }
    void *addresses[16];
    int errors=0;
    for (int i=0;i<n;i++) {
        addresses[i]=tls_addr(i);
        int *p=addresses[i], a=tls_alignment(i), want=tls_initial(i);
        int got=*p;
        int residue=(uintptr_t)p%a;
        if (got!=want || residue) errors++;
        pthread_mutex_lock(&lock);
        printf("thread=%d object=%d address=%p align=%d residue=%d initial=%d expected=%d\n",id,i,p,a,residue,got,want);
        pthread_mutex_unlock(&lock);
        *p=want+100+id;
    }
    pthread_mutex_lock(&lock);
    arrived++;
    pthread_cond_broadcast(&cond);
    while(arrived<3) pthread_cond_wait(&cond,&lock);
    pthread_mutex_unlock(&lock);
    for(int i=0;i<n;i++) {
        int *p=tls_addr(i);
        if(p!=addresses[i] || *p!=tls_initial(i)+100+id) errors++;
    }
    pthread_mutex_lock(&lock);
    failures+=errors;
    pthread_mutex_unlock(&lock);
}
static void *worker(void *p) {check((int)(intptr_t)p);return 0;}
int main(void) {
    pthread_t a,b;
    if(pthread_create(&a,0,worker,(void*)1) || pthread_create(&b,0,worker,(void*)2)) return 99;
    check(0);
    pthread_join(a,0);pthread_join(b,0);
    printf("failures=%d\n",failures);
    return failures?1:0;
}
