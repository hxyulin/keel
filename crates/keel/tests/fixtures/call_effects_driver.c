#include <signal.h>
#include <stdlib.h>
#include <sys/mman.h>
#include <sys/resource.h>
#include <sys/wait.h>
#include <unistd.h>
extern long effects_reuse(long *, long), effects_alias(long *, long *, long);
extern long effects_callback(long *, long, long (*)(long *, long));
extern long effects_order(long *, long *, long), effects_spill(long *, long);
static long mutate(long *p, long x) { *p = x - 3; return x + 7; }
static long revoke_page(long *p, long x) { if (mprotect(p, (size_t)getpagesize(), PROT_NONE)) _exit(99); return x; }
static int faulted(pid_t pid) {
 int status; if (waitpid(pid, &status, 0) != pid) return 0;
 return WIFSIGNALED(status) && (WTERMSIG(status) == SIGSEGV || WTERMSIG(status) == SIGBUS);
}
int main(void) {
 struct rlimit core = {0, 0}; setrlimit(RLIMIT_CORE, &core);
 for (long x = -100; x <= 100; ++x) {
  long p = x + 5;
  if (effects_reuse(&p, x) != 4*x + 13 || p != x + 5) return 1;
  if (effects_alias(&p, &p, x) != 3*x + 8 || p != x) return 2;
  p = x + 5;
  if (effects_callback(&p, x, mutate) != 3*x + 9 || p != x - 3) return 3;
  long a[40]; long sum = 0;
  for (long n = 0; n < 40; ++n) { a[n] = x + n; sum += a[n]; }
  if (effects_spill(a, x) != sum + 3*x + 3) return 4;
 }
 size_t page = (size_t)getpagesize();
 long *p = mmap(0, page, PROT_READ|PROT_WRITE, MAP_PRIVATE|MAP_ANON, -1, 0);
 long *marker = mmap(0, page, PROT_READ|PROT_WRITE, MAP_SHARED|MAP_ANON, -1, 0);
 if (p == MAP_FAILED || marker == MAP_FAILED) return 5;
 *p = 7; *marker = 0;
 pid_t child = fork();
 if (child < 0) return 6;
 if (!child) { mprotect(p, page, PROT_NONE); (void)effects_order(p, marker, 31); _exit(90); }
 if (!faulted(child) || *marker != 0) return 7; /* original read precedes mutation */
 child = fork();
 if (child < 0) return 8;
 if (!child) { (void)effects_callback(p, 31, revoke_page); _exit(91); }
 if (!faulted(child)) return 9; /* later read must fault after callback revokes access */
 munmap(p, page); munmap(marker, page);
 return 0;
}
