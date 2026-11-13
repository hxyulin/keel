#include <signal.h>
#include <stdint.h>
#include <sys/wait.h>
#include <unistd.h>

extern void trap_only(void);
extern uint64_t guarded_trap(volatile sig_atomic_t *, uint64_t);

static volatile sig_atomic_t observed;
static volatile sig_atomic_t expected;

static void trapped(int signal_number) {
#if defined(__aarch64__) || defined(__arm64__)
    const int trap_signal = SIGTRAP;
#else
    const int trap_signal = SIGILL;
#endif
    _exit(signal_number == trap_signal && observed == expected ? 0 : 3);
}

static int must_trap(int guarded) {
    pid_t child = fork();
    if (child < 0)
        return 1;
    if (!child) {
        observed = 0;
        expected = guarded ? 73 : 0;
        signal(SIGTRAP, trapped);
        signal(SIGILL, trapped);
        if (guarded)
            guarded_trap(&observed, UINT64_MAX);
        else
            trap_only();
        _exit(2);
    }
    int status;
    if (waitpid(child, &status, 0) != child)
        return 1;
    return !WIFEXITED(status) || WEXITSTATUS(status) != 0;
}

int main(void) {
    observed = 19;
    if (guarded_trap(&observed, 0) != 1 || observed != 19)
        return 1;
    if (must_trap(0) || must_trap(1))
        return 2;
    return 0;
}
