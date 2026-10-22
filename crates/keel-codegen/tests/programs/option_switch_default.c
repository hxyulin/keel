#include <stdint.h>
#include <stdio.h>

int64_t option_code(int64_t tag, int64_t payload);

struct option { int64_t tag, payload; };

/* Keep the value coming back from a real call, as in the Flux report. */
__attribute__((noinline))
static struct option pass(struct option value) { return value; }

int main(void) {
    const int64_t payloads[] = {
        0, 1, -1, 10, -10, INT64_MIN, INT64_MAX,
        INT64_C(0x123456789abcdef), -INT64_C(0x123456789abcdef)
    };
    for (int64_t tag = 0; tag <= 1; ++tag) {
        for (unsigned i = 0; i < sizeof(payloads) / sizeof(*payloads); ++i) {
            struct option value = pass((struct option){tag, payloads[i]});
            int64_t got = option_code(value.tag, value.payload);
            int64_t want = tag ? payloads[i] : -1;
            if (got != want) {
                fprintf(stderr, "tag=%lld payload=%lld: got=%lld want=%lld\n",
                        (long long)tag, (long long)payloads[i],
                        (long long)got, (long long)want);
                return 1;
            }
        }
    }
    struct option none = pass((struct option){0, 0});
    return option_code(none.tag, none.payload) + 10 == 9 ? 0 : 2;
}
