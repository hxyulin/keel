/* Linux x86-64 execution boundary for run.py. No writable executable mappings.
 */
#define _GNU_SOURCE
#include <cpuid.h>
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
static uint32_t wide_table[256];
static int dual_timer, intel_fences;
struct sample {
  uint64_t ns, ticks, result;
  unsigned aux_start, aux_end;
};

static inline void serialize(void) {
  unsigned a, b, c, d;
  __asm__ volatile("cpuid"
                   : "=a"(a), "=b"(b), "=c"(c), "=d"(d)
                   : "0"(0), "2"(0)
                   : "memory");
}

/* Intel documents LFENCE execution ordering. Other vendors use the more
   expensive CPUID boundary rather than assuming LFENCE dispatch serialization.
 */
static inline uint64_t timestamp(unsigned *aux) {
  unsigned lo, hi, id;
  if (intel_fences) {
    __asm__ volatile("lfence\n\trdtscp\n\tlfence"
                     : "=a"(lo), "=d"(hi), "=c"(id)
                     :
                     : "memory");
  } else {
    serialize();
    __asm__ volatile("rdtscp" : "=a"(lo), "=d"(hi), "=c"(id) : : "memory");
    serialize();
  }
  *aux = id;
  return ((uint64_t)hi << 32) | lo;
}
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

static struct sample measure(sequence_fn fn, uint64_t iterations, uint64_t seed,
                             const uint8_t *input) {
  struct sample s = {0};
  uint64_t wall_start = now(), tick_start = 0;
  if (dual_timer)
    tick_start = timestamp(&s.aux_start);
  s.result = fn(iterations, seed, input);
  if (dual_timer)
    s.ticks = timestamp(&s.aux_end) - tick_start;
  s.ns = now() - wall_start;
  return s;
}

static __attribute__((noinline)) uint64_t empty(uint64_t iterations,
                                                uint64_t seed,
                                                const uint8_t *input) {
  (void)iterations;
  (void)input;
  __asm__ volatile("" : : : "memory");
  return seed;
}

int main(int argc, char **argv) {
  if (argc != 3) {
    fprintf(stderr, "usage: driver CPU wall|dual\n");
    return 1;
  }
  char *end;
  long cpu = strtol(argv[1], &end, 10);
  if (*end || cpu < 0 || cpu >= CPU_SETSIZE)
    return 1;
  if (!strcmp(argv[2], "dual"))
    dual_timer = 1;
  else if (strcmp(argv[2], "wall"))
    return 1;
  cpu_set_t set;
  CPU_ZERO(&set);
  CPU_SET(cpu, &set);
  if (sched_setaffinity(0, sizeof(set), &set))
    fail("sched_setaffinity");
  unsigned a, b, c, d;
  unsigned max_leaf = __get_cpuid_max(0, NULL);
  unsigned max_extended = __get_cpuid_max(0x80000000, NULL);
  __cpuid(0, a, b, c, d);
  char vendor[13];
  memcpy(vendor, &b, 4);
  memcpy(vendor + 4, &d, 4);
  memcpy(vendor + 8, &c, 4);
  vendor[12] = 0;
  intel_fences = !strcmp(vendor, "GenuineIntel");
  int rdtscp = 0, invariant = 0;
  if (max_extended >= 0x80000001) {
    __cpuid(0x80000001, a, b, c, d);
    rdtscp = !!(d & (1u << 27));
  }
  if (max_extended >= 0x80000007) {
    __cpuid(0x80000007, a, b, c, d);
    invariant = !!(d & (1u << 8));
  }
  if (dual_timer && (!rdtscp || !invariant)) {
    fprintf(stderr, "dual timer requires RDTSCP and invariant TSC\n");
    return 1;
  }
  __cpuid(1, a, b, c, d);
  int hypervisor = !!(c & (1u << 31));
  uint64_t tsc_hz = 0;
  if (intel_fences && max_leaf >= 0x15) {
    __cpuid_count(0x15, 0, a, b, c, d);
    if (a && b && c)
      tsc_hz = (uint64_t)c * b / a;
  }
  for (unsigned i = 0; i < 256; ++i) {
    table[i] = (uint8_t)(i * 73 + 19);
    wide_table[i] = (uint32_t)table[i] | (((i * 11 + 3) & 255) << 8) |
                    (((i * 13 + 7) & 255) << 24);
  }
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
  printf("{\"vendor\":\"%s\",\"rdtscp\":%s,\"invariant_tsc\":%s,"
         "\"hypervisor\":%s,\"tsc_hz\":%" PRIu64 ",\"tsc_ordering\":\"%s\","
         "\"timer_probe\":[",
         vendor, rdtscp ? "true" : "false", invariant ? "true" : "false",
         hypervisor ? "true" : "false", tsc_hz,
         !dual_timer
             ? "disabled"
             : (intel_fences ? "LFENCE;RDTSCP;LFENCE" : "CPUID;RDTSCP;CPUID"));
  sequence_fn volatile empty_target = empty;
  for (unsigned i = 0; i < 32; ++i) {
    struct sample probe = measure(empty_target, 1, 73, table);
    if (dual_timer && probe.aux_start != probe.aux_end)
      return 1;
    printf("%s{\"ns\":%" PRIu64, i ? "," : "", probe.ns);
    if (dual_timer)
      printf(",\"tsc_ticks\":%" PRIu64, probe.ticks);
    printf("}");
  }
  printf("]}\n");
  fflush(stdout);
  /* Commands: blob path, placement index, iterations, seed, expected checksum,
     input kind. Each reply is one sample, after a separate untimed warmup. */
  char path[4096];
  unsigned placement, input_kind;
  uint64_t iterations, seed, expected;
  while (scanf("%4095s %u %" SCNu64 " %" SCNu64 " %" SCNu64 " %u", path,
               &placement, &iterations, &seed, &expected, &input_kind) == 6) {
    if (placement >= 3 || input_kind > 1)
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
    const uint8_t *input = input_kind ? (const uint8_t *)wide_table : table;
    volatile uint64_t warm = fn(1024, seed, input);
    (void)warm;
    struct sample s = measure(fn, iterations, seed, input);
    if (dual_timer && s.aux_start != s.aux_end) {
      fprintf(stderr, "TSC_AUX changed during sample\n");
      return 1;
    }
    if (s.result != expected) {
      fprintf(stderr, "checksum: got %" PRIu64 ", expected %" PRIu64 "\n",
              s.result, expected);
      return 1;
    }
    printf("{\"ns\":%" PRIu64 ",\"checksum\":%" PRIu64 ",\"entry\":%" PRIuPTR
           ",\"cpu\":%d,\"locked\":true",
           s.ns, s.result, (uintptr_t)entry, sched_getcpu());
    if (dual_timer)
      printf(",\"tsc_ticks\":%" PRIu64
             ",\"tsc_aux_start\":%u,\"tsc_aux_end\":%u",
             s.ticks, s.aux_start, s.aux_end);
    printf("}\n");
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
