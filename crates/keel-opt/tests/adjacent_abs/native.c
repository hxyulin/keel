// Correctness only: link with original and transformed adjacent IR functions.
#include <assert.h>
#include <limits.h>
#include <setjmp.h>
#include <signal.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/mman.h>
#include <unistd.h>

typedef uint64_t (*kernel)(const int32_t *, const void *, const void *, uint64_t);
extern uint64_t rk_adjacent(const int32_t *, const void *, const void *, uint64_t);
extern uint64_t rk_adjacent_before(const int32_t *, const void *, const void *, uint64_t);
static sigjmp_buf escape;
static volatile uintptr_t fault_address;
static volatile sig_atomic_t fault_signal;
static void fault(int sig, siginfo_t *info, void *unused) {
    (void)unused;
    fault_address = (uintptr_t)info->si_addr;
    fault_signal = sig;
    siglongjmp(escape, 1);
}
static void expect_fault(kernel fn, const int32_t *a, uintptr_t expected) {
    fault_signal = 0;
    if (sigsetjmp(escape, 1) == 0) {
        fn(a, NULL, NULL, 2);
        abort();
    }
    assert(fault_signal == SIGSEGV || fault_signal == SIGBUS);
    assert(fault_address == expected);
}
static uint64_t oracle(const int32_t *a, uint64_t n) {
    uint64_t sum = 0;
    for (uint64_t i = 1; i < n; ++i) {
        int64_t d = (int64_t)a[i] - (int64_t)a[i - 1];
        sum += (uint64_t)(d < 0 ? -d : d);
    }
    return sum;
}
int main(void) {
    size_t page = (size_t)sysconf(_SC_PAGESIZE);
    char *mem = mmap(NULL, 3 * page, PROT_NONE, MAP_PRIVATE | MAP_ANON, -1, 0);
    assert(mem != MAP_FAILED);
    assert(mprotect(mem + page, page, PROT_READ | PROT_WRITE) == 0);
    kernel funcs[] = {rk_adjacent_before, rk_adjacent};
    uint32_t state = 0x12345678;
    unsigned cases = 0;
    for (unsigned k = 0; k < 2; ++k) {
        for (unsigned i = 0; i < 2; ++i) {
            assert(funcs[k](NULL, NULL, NULL, i) == 0);
            assert(funcs[k]((int32_t *)mem, NULL, NULL, i) == 0);
            assert(funcs[k]((int32_t *)(UINTPTR_MAX - 3), NULL, NULL, i) == 0);
        }
        for (unsigned n = 0; n <= 257; ++n) {
            int32_t *a = (int32_t *)(mem + 2 * page) - n;
            for (unsigned trial = 0; trial < 8; ++trial) {
                for (unsigned i = 0; i < n; ++i) {
                    state ^= state << 13; state ^= state >> 17; state ^= state << 5;
                    a[i] = trial == 0 ? (i & 1 ? INT32_MIN : INT32_MAX) : (int32_t)state;
                }
                assert(funcs[k](a, NULL, NULL, n) == oracle(a, n));
                ++cases;
            }
        }
        // Both words fault: a[1] is the first word in the third page, a[0]
        // the last word in the second. Exact address identifies first-fault order.
        assert(mprotect(mem + page, page, PROT_NONE) == 0);
        struct sigaction action = {0};
        action.sa_sigaction = fault;
        action.sa_flags = SA_SIGINFO;
        sigemptyset(&action.sa_mask);
        assert(sigaction(SIGSEGV, &action, NULL) == 0);
        assert(sigaction(SIGBUS, &action, NULL) == 0);
        expect_fault(funcs[k], (int32_t *)(mem + 2 * page) - 1, (uintptr_t)(mem + 2 * page));
        // IR pointer additions wrap: the first a[1] address wraps to zero.
        expect_fault(funcs[k], (int32_t *)(UINTPTR_MAX - 3), 0);
        assert(mprotect(mem + page, page, PROT_READ | PROT_WRITE) == 0);
    }
    assert(munmap(mem, 3 * page) == 0);
    printf("{\"status\":\"passed\",\"array_cases\":%u,\"variants\":2,\"first_fault_order\":\"a[1] before a[0]\",\"timed\":false}\n", cases);
    return 0;
}
