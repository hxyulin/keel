static int check(const struct ShiftCase *c, const unsigned char *a,
                 const unsigned char *counts, unsigned char *expected) {
  for (int lane = 0; lane < c->bytes*8/c->bits; lane++) {
    uint64_t value = 0, count = 0;
    memcpy(&value, a+lane*c->bits/8, c->bits/8);
    if (c->immediate >= 0) count = c->immediate;
    else if (c->per_lane) memcpy(&count, counts+lane*c->bits/8, c->bits/8);
    else memcpy(&count, counts, 8);
    uint64_t result = shift_lane(c->op, c->bits, value, count);
    memcpy(expected+lane*c->bits/8, &result, c->bits/8);
  }
  unsigned char actual[32], vendor[32];
  c->keel(actual,a,counts);
  c->vendor(vendor,a,counts);
  return memcmp(actual,expected,c->bytes) || memcmp(vendor,expected,c->bytes);
}

int main(void) {
  long page = sysconf(_SC_PAGESIZE);
  unsigned char *maps[3];
  for (int k = 0; k < 3; k++) {
    maps[k] = mmap(0,page*3,PROT_NONE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);
    if (maps[k] == MAP_FAILED || mprotect(maps[k]+page,page,PROT_READ|PROT_WRITE)) return 1;
  }
  for (unsigned i = 0; i < sizeof(cases)/sizeof(*cases); i++) {
    const struct ShiftCase *c = &cases[i];
    uint64_t limits[] = {0,1,c->bits-1,c->bits,c->bits+1,255,256,
                        UINT64_C(1)<<32,UINT64_C(1)<<(c->bits-1),UINT64_MAX};
    for (int pattern = 0; pattern < 8; pattern++) {
      unsigned char a[32], counts[32], expected[32];
      uint64_t sign = UINT64_C(1) << (c->bits-1);
      for (int lane = 0; lane < c->bytes*8/c->bits; lane++) {
        uint64_t value = pattern == 0 ? 0 : pattern == 1 ? UINT64_MAX :
                         pattern == 2 ? sign : pattern == 3 ? sign-1 :
                         pattern == 4 ? 1 : pattern == 5 ? (lane%2 ? sign : 1) :
                         UINT64_C(0x9e3779b97f4a7c15)*(lane+pattern);
        memcpy(a+lane*c->bits/8,&value,c->bits/8);
      }
      unsigned count_cases = c->immediate >= 0 ? 1 : sizeof(limits)/sizeof(*limits);
      for (unsigned j = 0; j < count_cases; j++) {
        memset(counts, pattern%2 ? 0xff : 0, sizeof(counts));
        if (c->per_lane) {
          for (int lane = 0; lane < c->bytes*8/c->bits; lane++) {
            uint64_t count = limits[(j+lane)%10];
            memcpy(counts+lane*c->bits/8,&count,c->bits/8);
          }
        } else memcpy(counts,&limits[j],8);
        if (check(c,a,counts,expected)) {
          fprintf(stderr,"ABI/oracle: %s, immediate %d, pattern %d, count %u\n",c->name,c->immediate,pattern,j);
          return 2;
        }
        for (int edge = 0; edge < 3; edge++) {
          unsigned char *input = maps[0]+page+(edge == 0 ? 0 : edge == 1 ? page-c->bytes : 3);
          unsigned char *count = maps[1]+page+(edge == 0 ? 0 : edge == 1 ? page-c->count_bytes : 5);
          unsigned char *out = maps[2]+page+(edge == 0 ? 0 : edge == 1 ? page-c->bytes : 7);
          memcpy(input,a,c->bytes); memcpy(count,counts,c->count_bytes);
          memset(maps[2]+page,0xa5,page);
          kshift_evaluations = 0;
          c->memory(out,input,count);
          int evaluations = c->immediate >= 0 ? 1 : 2;
          if (kshift_evaluations != evaluations || memcmp(out,expected,c->bytes)) return 3;
          for (long k = 0; k < page; k++)
            if ((maps[2]+page+k < out || maps[2]+page+k >= out+c->bytes) && maps[2][page+k] != 0xa5) return 4;
          kshift_evaluations = 0;
          c->memory(input,input,count);
          if (kshift_evaluations != evaluations || memcmp(input,expected,c->bytes)) return 5;
          /* Wide output cannot fit over a 16-byte count at the page's end. */
          if (edge != 1 || c->bytes <= c->count_bytes) {
            memcpy(input,a,c->bytes); memcpy(count,counts,c->count_bytes);
            kshift_evaluations = 0;
            c->memory(count,input,count);
            if (kshift_evaluations != evaluations || memcmp(count,expected,c->bytes)) return 6;
          }
        }
      }
    }
  }
  for (int k = 0; k < 3; k++) munmap(maps[k],page*3);
  puts("PASS: AVX2 uniform/immediate/per-lane shifts, vector ABI, scalar/vendor oracles, footprints and aliasing");
  return 0;
}
