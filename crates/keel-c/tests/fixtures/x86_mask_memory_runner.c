extern void *memcpy(void *, const void *, unsigned long);
extern int memcmp(const void *, const void *, unsigned long);
extern int printf(const char *, ...);
extern long sysconf(int);
extern void *mmap(void *, unsigned long, int, int, int, long);
extern int mprotect(void *, unsigned long, int);
extern int munmap(void *, unsigned long);
static void bits(U *p, Q value, unsigned bytes) {
  for (unsigned j = 0; j < bytes; ++j)
    p[j] = (U)(value >> (8 * j));
}

int main(void) {
  unsigned long page = (unsigned long)sysconf(30);
  if (page < 4096)
    return 90;
  U *mapping = mmap(0, page * 3, 3, 0x22, -1, 0),
    *reference_mapping = mmap(0, page * 3, 3, 0x22, -1, 0);
  if (mapping == (void *)-1 || reference_mapping == (void *)-1)
    return 91;
  if (mprotect(mapping, page, 0) || mprotect(mapping + 2 * page, page, 0) ||
      mprotect(reference_mapping, page, 0) ||
      mprotect(reference_mapping + 2 * page, page, 0))
    return 92;
  U *memory = mapping + page, *reference_memory = reference_mapping + page;
  U mask[32], data[32], actual[48], reference[32], want[32], indices[32],
      source[32], alias[32];
  for (unsigned j = 0; j < 32; ++j) {
    data[j] = (U)(j * 79 + 137);
    source[j] = (U)(j * 31 + 19);
  }
  for (unsigned c = 0; c < sizeof(memory_cases) / sizeof(memory_cases[0]);
       ++c) {
    const struct MemoryCase *test = &memory_cases[c];
    unsigned lanes = test->bytes / test->lane;
    for (unsigned mode = 0; mode < 3; ++mode) {
      unsigned active = mode == 0 ? 0 : mode == 1 ? 1 : lanes - 1;
      for (unsigned j = 0; j < page; ++j)
        memory[j] = reference_memory[j] = (U)(j * 17 + 93);
      for (unsigned j = 0; j < lanes; ++j)
        bits(mask + j * test->lane,
             j < active ? (1ULL << (test->lane * 8 - 1)) | 1
                        : (1ULL << (test->lane * 8 - 1)) - 1,
             test->lane);
      U *base = memory + page - active * test->lane,
        *reference_base = reference_memory + page - active * test->lane;
      for (unsigned j = 0; j < test->bytes; ++j)
        want[j] = j < active * test->lane ? base[j] : 0;
      for (unsigned j = 0; j < 48; ++j)
        actual[j] = 0xa5;
      test->actual(actual + 8, base, base, mask, data);
      test->reference(reference, reference_base, reference_base, mask, data);
      if (memcmp(actual + 8, want, test->bytes) ||
          memcmp(reference, want, test->bytes) ||
          memcmp(memory, reference_memory, page)) {
        printf("mask memory mismatch %u mode %u\n", c, mode);
        return 1;
      }
      for (unsigned j = 0; j < active * test->lane; ++j)
        if (base[j] != data[j])
          return 2;
      for (unsigned j = 0; j < 8; ++j)
        if (actual[j] != 0xa5 || actual[8 + test->bytes + j] != 0xa5)
          return 3;
    }
  }
  for (unsigned c = 0; c < sizeof(gather_cases) / sizeof(gather_cases[0]);
       ++c) {
    const struct GatherCase *test = &gather_cases[c];
    unsigned lanes = test->bytes / test->lane,
             indexlanes = test->indexbytes / test->indexlane;
    unsigned active_lanes = lanes < indexlanes ? lanes : indexlanes;
    for (unsigned j = 0; j < page; ++j)
      memory[j] = (U)(j * 43 + 109);
    const U *base = memory + page / 2;
    for (unsigned mode = 0; mode < 3; ++mode) {
      for (unsigned j = 0; j < 32; ++j)
        indices[j] = mask[j] = 0;
      for (unsigned j = 0; j < lanes; ++j) {
        int active = !test->masked || (mode == 1 ? j % 2 == 0 : mode == 2);
        long index = (long)j * 7 - 11;
        if (j < active_lanes) {
          if (!active)
            index = (long)(page / 2 / test->scale) + 1;
          bits(indices + j * test->indexlane, (Q)index, test->indexlane);
        }
        bits(mask + j * test->lane,
             active ? (1ULL << (test->lane * 8 - 1)) | 1
                    : (1ULL << (test->lane * 8 - 1)) - 1,
             test->lane);
        for (unsigned k = 0; k < test->lane; ++k)
          want[j * test->lane + k] = j >= active_lanes ? 0
                                     : active
                                         ? base[index * (long)test->scale + k]
                                         : source[j * test->lane + k];
      }
      /* An all-inactive gather may use a completely inaccessible base. */
      const U *address = test->masked && mode == 0 ? mapping : base;
      for (unsigned j = 0; j < 48; ++j)
        actual[j] = 0xa5;
      test->actual(actual + 8, address, indices, source, mask);
      test->reference(reference, address, indices, source, mask);
      if (memcmp(actual + 8, want, test->bytes) ||
          memcmp(reference, want, test->bytes)) {
        printf("gather mismatch %u mode %u\n", c, mode);
        return 4;
      }
      for (unsigned j = 0; j < 8; ++j)
        if (actual[j] != 0xa5 || actual[8 + test->bytes + j] != 0xa5)
          return 5;
      memcpy(alias, source, 32);
      test->actual(alias, address, indices, alias, mask);
      if (memcmp(alias, want, test->bytes))
        return 6;
      memcpy(alias, mask, 32);
      test->actual(alias, address, indices, source, alias);
      if (memcmp(alias, want, test->bytes))
        return 7;
    }
  }
  if (munmap(mapping, page * 3) || munmap(reference_mapping, page * 3))
    return 93;
  return 0;
}
