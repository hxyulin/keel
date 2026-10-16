/* Native correctness only; no timing. Link once with Keel, once with Clang. */
#include <stdio.h>
#include <stdlib.h>
#include <signal.h>
#include <sys/mman.h>
#include <sys/resource.h>
#include <sys/wait.h>
#include <unistd.h>

unsigned long kernel(unsigned long *, unsigned long *, unsigned long, unsigned long);
unsigned long aliasing(unsigned long *, unsigned long *, unsigned long);
unsigned long conditional(unsigned long *, unsigned long, unsigned long);
unsigned long readonly_alias(unsigned long *, unsigned long *, unsigned long);
unsigned long fault_order(unsigned long *, unsigned long *, unsigned long);

int main(void) {
    const unsigned long trips[] = {0, 1, 7, 257, 4096};
    for (unsigned j = 0; j < sizeof(trips) / sizeof(trips[0]); ++j) {
        for (unsigned long seed = 0; seed < 16; ++seed) {
            unsigned long p = seed * 123456789UL + 17, q = 99;
            unsigned long result = kernel(&p, &q, trips[j], seed);
            printf("%lu %lu %lu %lu\n", trips[j], seed, result, q);
            result = aliasing(&p, &p, trips[j]);
            printf("alias %lu %lu\n", result, p);
            printf("readonly %lu\n", readonly_alias(&p, &p, trips[j]));
        }
    }
    long page = sysconf(_SC_PAGESIZE);
    if (page <= 0) return 10;
    void *protected = mmap(NULL, (size_t)page, PROT_NONE,
                           MAP_PRIVATE | MAP_ANON, -1, 0);
    unsigned long *shared = mmap(NULL, (size_t)page, PROT_READ | PROT_WRITE,
                                 MAP_SHARED | MAP_ANON, -1, 0);
    if (protected == MAP_FAILED || shared == MAP_FAILED) return 11;
    if (kernel(NULL, NULL, 0, 5) || kernel(protected, NULL, 0, 5)) return 12;
    if (conditional(NULL, 7, 0) || conditional(protected, 7, 0)) return 13;
    /* A preceding store must remain visible before the invalid load faults. */
    *shared = 0;
    struct rlimit limit = {0, 0};
    if (setrlimit(RLIMIT_CORE, &limit)) return 14;
    pid_t child = fork();
    if (child < 0) return 15;
    if (child == 0) {
        (void)fault_order(protected, shared, 1);
        _exit(16);
    }
    int status = 0;
    if (waitpid(child, &status, 0) != child || !WIFSIGNALED(status)) return 17;
    if (WTERMSIG(status) != SIGSEGV && WTERMSIG(status) != SIGBUS) return 18;
    if (*shared != 123) return 19;
    puts("zero-trip conditional fault-order ok");
    if (munmap(protected, (size_t)page) || munmap(shared, (size_t)page)) return 20;
    return 0;
}
