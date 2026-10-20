#include <stdint.h>
#include <stdio.h>
#include <sys/wait.h>
#include <unistd.h>
#include <signal.h>

int32_t sparse_default(int32_t, int32_t *, int32_t *);
int32_t trap_default(int32_t);

int main(void) {
    const int32_t inputs[] = {-1, 0, 258, 259, 260, 261, 287, 288, 289};
    for (unsigned i = 0; i < sizeof(inputs) / sizeof(*inputs); ++i) {
        int32_t x = inputs[i];
        int32_t want = x == 259 ? 17 : x == 260 ? 29 : x == 288 ? 288 : 41;
        for (int alias = 0; alias <= 1; ++alias) {
            int32_t stored = -9, other = 73;
            int32_t result = sparse_default(x, &stored, alias ? &stored : &other);
            if (stored != want || result != (alias ? want : other)) {
                fprintf(stderr, "switch x=%d alias=%d: stored=%d result=%d\n",
                        x, alias, stored, result);
                return 1;
            }
        }
    }
    if (trap_default(259) != 259 || trap_default(288) != 288) return 2;
    pid_t child = fork();
    if (child < 0) return 3;
    if (child == 0) { (void)trap_default(0); _exit(4); }
    int status;
    if (waitpid(child, &status, 0) != child || !WIFSIGNALED(status)) return 5;
    int sig = WTERMSIG(status);
    return sig == SIGILL || sig == SIGTRAP ? 0 : 6;
}
