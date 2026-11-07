#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>
extern void k_matmul(const int32_t *, const int32_t *, int32_t *, int32_t);
struct Region {
  void *base;
  size_t size;
  uint32_t *p;
};
static struct Region region(size_t cells) {
  size_t page = (size_t)sysconf(_SC_PAGESIZE), bytes = cells * 4,
         rounded = (bytes + page - 1) / page * page;
  void *p =
      mmap(0, rounded + 2 * page, PROT_NONE, MAP_PRIVATE | MAP_ANON, -1, 0);
  if (p == MAP_FAILED ||
      mprotect((char *)p + page, rounded, PROT_READ | PROT_WRITE))
    exit(2);
  return (struct Region){p, rounded + 2 * page,
                         (uint32_t *)((char *)p + page + rounded - bytes)};
}
static void reference(uint32_t *a, uint32_t *b, uint32_t *c, int n) {
  for (int i = 0; i < n; i++)
    for (int j = 0; j < n; j++) {
      uint32_t s = 0;
      for (int k = 0; k < n; k++)
        s += a[i * n + k] * b[k * n + j];
      c[i * n + j] = s;
    }
}
int main(void) {
  k_matmul(0, 0, 0, 0);
  k_matmul(0, 0, 0, -1);
  int dims[] = {1, 2, 3, 4, 5, 7, 8, 9, 15, 16, 17, 31, 32, 33};
  unsigned checks = 0;
  for (unsigned z = 0; z < sizeof(dims) / sizeof(*dims); z++) {
    int n = dims[z];
    size_t cells = (size_t)n * n + 2, bytes = cells * 4;
    for (unsigned alias = 0; alias < 6; alias++) {
      struct Region a = region(cells), b = region(cells), c = region(cells);
      uint32_t *aa = malloc(bytes), *bb = malloc(bytes), *cc = malloc(bytes);
      if (!aa || !bb || !cc)
        return 2;
      for (size_t i = 0; i < cells; i++) {
        a.p[i] = aa[i] =
            (uint32_t)(UINT64_C(0x9e3779b97f4a7c15) * i + UINT64_C(0x80000000));
        b.p[i] = bb[i] = (uint32_t)(UINT64_C(0x5851f42d4c957f2d) * i + 3);
        c.p[i] = cc[i] = 0xdeadbeef;
      }
      uint32_t *out = alias == 1   ? a.p
                      : alias == 2 ? a.p + 1
                      : alias == 3 ? b.p
                      : alias == 4 ? b.p + 1
                                   : c.p,
               *expected = alias == 1   ? aa
                           : alias == 2 ? aa + 1
                           : alias == 3 ? bb
                           : alias == 4 ? bb + 1
                                        : cc;
      uint32_t *right = alias == 5 ? a.p : b.p,
               *ref_right = alias == 5 ? aa : bb;
      reference(aa, ref_right, expected, n);
      k_matmul((int32_t *)a.p, (int32_t *)right, (int32_t *)out, n);
      if (memcmp(a.p, aa, bytes) || memcmp(b.p, bb, bytes) ||
          memcmp(c.p, cc, bytes)) {
        fprintf(stderr, "matrix mismatch n=%d alias=%u\n", n, alias);
        return 1;
      }
      checks++;
      free(aa);
      free(bb);
      free(cc);
      munmap(a.base, a.size);
      munmap(b.base, b.size);
      munmap(c.base, c.size);
    }
  }
  printf("%u guarded matrix/alias/wrapping cases and null empty trips passed\n",
         checks);
}
