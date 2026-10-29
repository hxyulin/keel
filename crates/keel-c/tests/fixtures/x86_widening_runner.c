int main(void) {
  for (unsigned c = 0; c < sizeof(cases) / sizeof(cases[0]); c++) {
    const struct Case *t = &cases[c];
    unsigned input_bytes = t->input_bits / 8, output_bytes = t->output_bits / 8;
    uint64_t sign = 1ull << (t->input_bits - 1);
    uint64_t mask = (sign << 1) - 1;
    uint64_t edges[] = {0, 1, sign - 1, sign, sign + 1, mask};
    for (unsigned test = 0; test < 70; test++) {
      unsigned char input[40], actual[40], reference[40], expected[40], alias[40];
      for (unsigned i = 0; i < 40; i++)
        input[i] = actual[i] = reference[i] = expected[i] = alias[i] = 0xa5;
      for (unsigned lane = 0; lane < 16 / input_bytes; lane++) {
        uint64_t value = test < 6 ? edges[(test + lane) % 6] : random_word() & mask;
        write_lane(input + 1 + lane * input_bytes, input_bytes, value);
      }
      for (unsigned lane = 0; lane < t->bytes / output_bytes; lane++) {
        uint64_t value = read_lane(input + 1 + lane * input_bytes, input_bytes);
        if (t->signed_input && (value & sign)) value |= ~mask;
        write_lane(expected + 1 + lane * output_bytes, output_bytes, value);
      }
      t->keel(input + 1, actual + 1);
      t->vendor(input + 1, reference + 1);
      for (unsigned i = 0; i < 16; i++) alias[i + 1] = input[i + 1];
      t->keel(alias + 1, alias + 1);
      for (unsigned i = 0; i < 40; i++) {
        if (actual[i] != expected[i] || reference[i] != expected[i] || alias[i] != expected[i]) {
          printf("%s test %u byte %u: keel=%u vendor=%u scalar=%u alias=%u\n",
                 t->name, test, i, actual[i], reference[i], expected[i], alias[i]);
          return 1;
        }
      }
    }
  }
  return 0;
}
