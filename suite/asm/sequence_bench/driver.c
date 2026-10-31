/* Linux x86-64 execution boundary for run.py. No writable executable mappings.
 */
#define _GNU_SOURCE
#include <errno.h>
#include <inttypes.h>
#include <sched.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <time.h>
#include <unistd.h>

typedef uint64_t (*sequence_fn)(uint64_t, uint64_t, const uint8_t *);
static uint8_t table[256];
static uint64_t now(void) {
  struct timespec t;
  if (clock_gettime(CLOCK_MONOTONIC_RAW, &t)) {
    perror("clock_gettime");
    exit(1);
  }
  return (uint64_t)t.tv_sec * 1000000000 + t.tv_nsec;
}
static void fail(const char *what) {
  perror(what);
  exit(1);
}

int main(int argc, char **argv) {
  if (argc != 2) {
    fprintf(stderr, "usage: driver CPU\n");
    return 1;
  }
  char *end;
  long cpu = strtol(argv[1], &end, 10);
  if (*end || cpu < 0 || cpu >= CPU_SETSIZE)
    return 1;
  cpu_set_t set;
  CPU_ZERO(&set);
  CPU_SET(cpu, &set);
  if (sched_setaffinity(0, sizeof(set), &set))
    fail("sched_setaffinity");
  for (unsigned i = 0; i < 256; ++i)
    table[i] = (uint8_t)(i * 73 + 19);
  const size_t capacity = 65536;
  uint8_t *arenas[3];
  for (unsigned i = 0; i < 3; ++i) {
    arenas[i] = mmap(NULL, capacity, PROT_READ | PROT_WRITE,
                     MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (arenas[i] == MAP_FAILED)
      fail("mmap");
    memset(arenas[i], 0xcc, capacity);
    if (mlock(arenas[i], capacity))
      fail("mlock");
  }
  /* Commands: blob path, placement index, iterations, seed, expected checksum.
     Each reply is one sample, after a separate untimed warmup. */
  char path[4096];
  unsigned placement;
  uint64_t iterations, seed, expected;
  while (scanf("%4095s %u %" SCNu64 " %" SCNu64 " %" SCNu64, path, &placement,
               &iterations, &seed, &expected) == 5) {
    if (placement >= 3)
      return 1;
    FILE *f = fopen(path, "rb");
    if (!f)
      fail("fopen");
    if (fseek(f, 0, SEEK_END))
      fail("fseek");
    long size = ftell(f);
    if (size <= 0 || (size_t)size > capacity - 64)
      return 1;
    rewind(f);
    uint8_t *arena = arenas[placement], *entry = arena + placement * 16;
    if (mprotect(arena, capacity, PROT_READ | PROT_WRITE))
      fail("mprotect RW");
    if (fread(entry, 1, (size_t)size, f) != (size_t)size)
      return 1;
    fclose(f);
    __builtin___clear_cache((char *)entry, (char *)entry + size);
    if (mprotect(arena, capacity, PROT_READ | PROT_EXEC))
      fail("mprotect RX");
    sequence_fn fn = (sequence_fn)(void *)entry;
    volatile uint64_t warm = fn(1024, seed, table);
    (void)warm;
    uint64_t start = now();
    uint64_t result = fn(iterations, seed, table);
    uint64_t elapsed = now() - start;
    if (result != expected) {
      fprintf(stderr, "checksum: got %" PRIu64 ", expected %" PRIu64 "\n",
              result, expected);
      return 1;
    }
    printf("{\"ns\":%" PRIu64 ",\"checksum\":%" PRIu64 ",\"entry\":%" PRIuPTR
           ",\"cpu\":%d,\"locked\":true}\n",
           elapsed, result, (uintptr_t)entry, sched_getcpu());
    fflush(stdout);
  }
  if (!feof(stdin))
    return 1;
  for (unsigned i = 0; i < 3; ++i) {
    if (munmap(arenas[i], capacity))
      fail("munmap");
  }
  return 0;
}
