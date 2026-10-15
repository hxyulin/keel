/* Catch the debugger trap and resume, so code after it must still run. */
#define _POSIX_C_SOURCE 200809L
#define _XOPEN_SOURCE 700
#include <signal.h>
#include <stdio.h>
#include <ucontext.h>

static volatile sig_atomic_t stops;

static void resume(int signal, siginfo_t *info, void *context) {
    stops++;
#if defined(__APPLE__) && defined(__aarch64__)
    /* ARM64 saves the PC of brk; x86-64 int3 saves the next PC. */
    ucontext_t *state = context;
    state->uc_mcontext->__ss.__pc += 4;
#endif
}

int main(void) {
    struct sigaction action = { 0 };
    action.sa_sigaction = resume;
    action.sa_flags = SA_SIGINFO;
    sigemptyset(&action.sa_mask);
    if (sigaction(SIGTRAP, &action, 0) != 0) return 1;

    int value = 40;
    __builtin_debugtrap();
    value++;
    __builtin_debugtrap();
    value++;
    printf("resumed %d %d\n", value, (int)stops);
    return value == 42 && stops == 2 ? 0 : 2;
}
