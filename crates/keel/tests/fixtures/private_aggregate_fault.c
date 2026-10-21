/* Distinct protected source pages test Keel's observable fault order, rather
 * than C by-value behavior for inaccessible objects. A high-word case puts the
 * low word in a readable page immediately before the protected page. */
#include <sys/mman.h>
#include <unistd.h>
#include <signal.h>
#include <stdint.h>
#include <stdlib.h>
static uintptr_t a, z;
static size_t pagesize;
extern long entry(void *, void *, void *);
static void fault(int sig, siginfo_t *info, void *context) {
    (void)sig; (void)context;
    uintptr_t x = (uintptr_t)info->si_addr;
    if (x >= a && x - a < pagesize) _Exit(11);
    if (x >= z && x - z < pagesize) _Exit(15);
    _Exit(99);
}
int main(int argc, char **argv) {
    if (argc != 2) return 95;
    int mode = atoi(argv[1]);
    pagesize = (size_t)sysconf(_SC_PAGESIZE);
    char *mapping = mmap(0, 4*pagesize, PROT_READ|PROT_WRITE,
                         MAP_PRIVATE|MAP_ANON, -1, 0);
    if (mapping == MAP_FAILED) return 98;
    a = (uintptr_t)mapping + pagesize;
    z = (uintptr_t)mapping + 3*pagesize;
    if (mprotect((void *)a, pagesize, PROT_NONE) ||
        mprotect((void *)z, pagesize, PROT_NONE)) return 94;
    struct sigaction sa = {0};
    sa.sa_sigaction = fault; sa.sa_flags = SA_SIGINFO;
    sigemptyset(&sa.sa_mask);
    if (sigaction(SIGSEGV, &sa, 0) || sigaction(SIGBUS, &sa, 0)) return 97;
    long good[2] = {7, 9};
    void *first = mode == 1 ? good : (void *)(a - (mode == 2 ? 8 : 0));
    void *last = (void *)(z - (mode == 3 ? 8 : 0));
    (void)entry(first, good, last);
    return 96;
}
