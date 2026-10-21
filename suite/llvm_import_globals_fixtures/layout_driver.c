#include <assert.h>
#include <stdint.h>
#include <stddef.h>
#include <stdio.h>
struct Pair { int16_t a; int64_t b; };
struct R { uint8_t tag; struct Pair pairs[2]; struct R *self; };
extern struct R nested, zero_record;
extern struct __attribute__((packed)) { struct R *p; uint8_t x; } packed_aligned;
extern struct __attribute__((packed)) { uint8_t x; void *p; } packed_null;
extern const uint8_t bits[3];
extern struct R *const one_past;
extern int64_t *const nested_field;
extern int32_t *external_element;
extern int64_t *private_pointer;
int32_t external_array[4] = {2, 3, 5, 7};
const int32_t external_const = -701;
extern int64_t *constant_expression(void);
extern int32_t read_external_const(void);
extern void *pointer_array[3], *max_addend;
extern void *max_addend_address(void);
int main(void) {
    assert(sizeof(struct R) == 48 && offsetof(struct R, self) == 40);
    assert((uintptr_t)&nested % 16 == 0);
    assert((uintptr_t)&packed_aligned % 8 == 0 && packed_aligned.p == &nested && packed_aligned.x == 21);
    assert(packed_null.x == 22 && packed_null.p == 0);
    assert(nested.tag == 7 && nested.self == &nested);
    assert(nested.pairs[0].a == -11 && nested.pairs[0].b == 13);
    assert(nested.pairs[1].a == 17 && nested.pairs[1].b == -19);
    assert(zero_record.tag == 0 && zero_record.self == 0);
    for (int n=0; n<2; n++) assert(zero_record.pairs[n].a == 0 && zero_record.pairs[n].b == 0);
    assert(bits[0] == 1 && bits[1] == 0 && bits[2] == 1);
    assert(one_past == &nested + 1 && nested_field == &nested.pairs[1].b);
    assert(constant_expression() == nested_field);
    assert(external_element == external_array + 3 && *external_element == 7);
    *external_element = -23; assert(external_array[3] == -23);
    assert(*private_pointer == -77); *private_pointer = 79; assert(*private_pointer == 79);
    assert(read_external_const() == -701);
    assert(pointer_array[0] == &nested && pointer_array[1] == 0 && pointer_array[2] == &nested + 1);
    assert((uintptr_t)max_addend == (uintptr_t)external_array + UINT64_C(4294967295));
    assert(max_addend_address() == max_addend);
    puts("nested layout assertions passed");
}
