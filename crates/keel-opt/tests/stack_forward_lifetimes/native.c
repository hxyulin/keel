#include <stdint.h>
extern int64_t copy_value(void *, int64_t, int64_t);
extern int64_t sub_values(int64_t, int64_t, int64_t);
void effect(void *counter) { ++*(unsigned *)counter; }
int main(void) {
    const int64_t payloads[] = {0, 1, -1, 2147483647, 2147483648LL, 0x1234567887654321LL};
    const int64_t tags[] = {0, 1, -7, 42};
    unsigned counter = 0;
    for (unsigned i = 0; i < sizeof(payloads) / sizeof(*payloads); ++i)
        for (unsigned j = 0; j < sizeof(tags) / sizeof(*tags); ++j) {
            int64_t expected = (int64_t)(int32_t)payloads[i] + tags[j];
            if (sub_values(payloads[i], tags[j], 0) != (int64_t)(int32_t)payloads[i] - (int64_t)(int32_t)tags[j])
                return 3;
            if (sub_values(payloads[i], tags[j], 1) != payloads[i])
                return 4;
            if (copy_value(&counter, payloads[i], tags[j]) != expected)
                return 1;
        }
    return counter == 24 ? 0 : 2;
}
