/* Correctness only. No timers. macOS arm64 native; both-target objects are
 * validated separately. Protected memory tests apply only to generic_load,
 * whose original potentially faulting wide access must remain unchanged. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <signal.h>
#include <sys/mman.h>
#include <sys/resource.h>
#include <sys/wait.h>
#include <unistd.h>

extern uint64_t stack_o0(uint64_t), stack_o1(uint64_t), stack_o8(uint64_t);
extern uint64_t stack_o16(uint64_t), stack_mayfault(uint64_t), stack_debug(uint64_t);
extern uint64_t stack_write(uint64_t, uint64_t), generic_load(const void *);
extern uint64_t bridge_narrow_load(uint64_t, uint64_t, const uint64_t *, uint64_t);
extern uint64_t oracle_stack(uint64_t, unsigned), oracle_write(uint64_t, uint64_t);
extern uint64_t oracle_generic(const void *);
extern uint64_t oracle_bridge(uint64_t, uint64_t, const uint64_t *, uint64_t);

static unsigned comparisons;
static void equal(uint64_t actual, uint64_t expected) {
    ++comparisons;
    if (actual != expected) {
        fprintf(stderr, "comparison %u: got %llu expected %llu\n", comparisons,
                (unsigned long long)actual, (unsigned long long)expected);
        exit(1);
    }
}

static int must_fault(uint64_t (*function)(const void *), const void *address) {
    pid_t child = fork();
    if (child < 0) return 1;
    if (child == 0) {
        struct rlimit zero = {0, 0};
        if (setrlimit(RLIMIT_CORE, &zero) != 0) _exit(2);
        volatile uint64_t value = function(address);
        (void)value;
        _exit(3);
    }
    int status;
    if (waitpid(child, &status, 0) != child) return 1;
    if (!WIFSIGNALED(status)) return 1;
    return WTERMSIG(status) != SIGSEGV && WTERMSIG(status) != SIGBUS;
}

int main(void) {
    const uint64_t high[] = {0, UINT64_C(0xffffffffffffff00),
        UINT64_C(0x8000000000000000), UINT64_C(0x123456789abcde00)};
    for (unsigned byte = 0; byte < 256; ++byte) {
        for (unsigned h = 0; h < sizeof high / sizeof high[0]; ++h) {
            uint64_t value = high[h] | byte;
            equal(stack_o0(value), oracle_stack(value, 0));
            equal(stack_o1(value), oracle_stack(value, 1));
            equal(stack_o8(value), oracle_stack(value, 8));
            equal(stack_o16(value), oracle_stack(value, 16));
            equal(stack_mayfault(value), oracle_stack(value, 0));
            equal(stack_debug(value), oracle_stack(value, 0));
            equal(stack_write(value, ~value), oracle_write(value, ~value));
            unsigned char unaligned[16];
            memcpy(unaligned + 1, &value, sizeof value);
            equal(generic_load(unaligned + 1), oracle_generic(unaligned + 1));
        }
    }
    uint64_t data[256];
    for (unsigned i = 0; i < 256; ++i)
        data[i] = (UINT64_C(0xfedcba9876543200) | (i * 71u % 256u));
    const uint64_t seeds[] = {0, 1, 127, 128, 255, 256, UINT64_MAX,
        UINT64_C(0x8000000000000000)};
    const uint64_t masks[] = {0, 1, 15, 255};
    const uint64_t counts[] = {0, 1, 2, 3, 17, 33};
    for (unsigned s = 0; s < sizeof seeds / sizeof seeds[0]; ++s) {
        equal(bridge_narrow_load(0, seeds[s], NULL, UINT64_MAX),
              oracle_bridge(0, seeds[s], NULL, UINT64_MAX));
        for (unsigned m = 0; m < sizeof masks / sizeof masks[0]; ++m)
            for (unsigned n = 0; n < sizeof counts / sizeof counts[0]; ++n)
                equal(bridge_narrow_load(counts[n], seeds[s], data, masks[m]),
                      oracle_bridge(counts[n], seeds[s], data, masks[m]));
    }

    long page = sysconf(_SC_PAGESIZE);
    if (page <= 0) return 2;
    unsigned char *memory = mmap(NULL, (size_t)page * 2, PROT_READ | PROT_WRITE,
                                 MAP_PRIVATE | MAP_ANON, -1, 0);
    if (memory == MAP_FAILED) return 2;
    memory[page - 1] = 255;
    if (mprotect(memory + page, (size_t)page, PROT_NONE) != 0) return 2;
    equal(memory[page - 1], 255);
    /* Both calls request eight bytes, of which only the first is accessible.
     * No notrap promise is made for this rejected generic-pointer case. */
    if (must_fault(generic_load, memory + page - 1) ||
        must_fault(oracle_generic, memory + page - 1)) return 3;
    if (munmap(memory, (size_t)page * 2) != 0) return 2;
    printf("PASS comparisons=%u boundary_faults=2\n", comparisons);
    return 0;
}
