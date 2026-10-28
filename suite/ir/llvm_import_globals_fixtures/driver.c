#include <assert.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
struct Record { uint8_t tag; int64_t wide; int16_t tail; };
struct __attribute__((packed)) Packed { uint8_t tag; int32_t value; };
struct PointerRecord { uint8_t tag; int32_t *ptr; int16_t *field; };
extern int32_t zeroes[7], writable;
extern const uint16_t constants[4];
extern const char text[5];
extern struct Record record;
extern struct Packed packed;
extern struct PointerRecord pointer_record;
extern int32_t *data_pointer, *element_pointer, *external_pointer, *null_pointer;
extern int16_t *field_pointer;
extern int32_t (*function_pointer)(void), (*external_function_pointer)(void);
extern int32_t *local_address(void), *element_address(void), *external_address(void);
extern int32_t (*function_address(void))(void);
extern int32_t target(void), read_local(void);
extern void replace_writable(int32_t);
extern int32_t (*local_function_pointer)(void);
extern int32_t (*local_function_address(void))(void);
int32_t external_value = 901;
int32_t external_function(void) { return -89; }
int main(void) {
    assert(sizeof(struct Record) == 24 && offsetof(struct Record, wide) == 8 && offsetof(struct Record, tail) == 16);
    assert(((uintptr_t)&record % _Alignof(struct Record)) == 0);
    assert(record.tag == 0xa5 && record.wide == INT64_C(-72623859790382856) && record.tail == -1234);
    assert(packed.tag == 0x7e && packed.value == 0x12345678 && sizeof(packed) == 5);
    assert(constants[0] == 0 && constants[1] == 1 && constants[2] == 32768 && constants[3] == 65535);
    assert(text[0] == 'a' && text[1] == 0 && (unsigned char)text[2] == 255 && text[3] == '\n' && text[4] == 'z');
    for (int n = 0; n < 7; ++n) { assert(zeroes[n] == 0); zeroes[n] = n + 8; }
    assert(zeroes[6] == 14 && null_pointer == 0 && writable == -31);
    assert(data_pointer == &writable); *data_pointer = 1003; assert(writable == 1003);
    replace_writable(-1004); assert(writable == -1004 && *data_pointer == -1004);
    int32_t *local = local_address();
    assert(read_local() == -9);
    assert(local_function_address() == local_function_pointer && local_function_pointer() == 101);
    assert(local[0] == 17 && local[1] == -9 && local[2] == 42);
    assert(element_address() == local + 2 && element_pointer == local + 2);
    *element_pointer = -111; assert(local_address()[2] == -111);
    assert(field_pointer == &record.tail); *field_pointer = 99; assert(record.tail == 99);
    assert(external_pointer == &external_value && external_address() == &external_value);
    *external_pointer = -902; assert(external_value == -902);
    assert(function_pointer == target && function_address() == target && function_pointer() == 73);
    assert(external_function_pointer == external_function && external_function_pointer() == -89);
    assert(pointer_record.tag == 11 && pointer_record.ptr == local + 1 && pointer_record.field == &record.tail);
    puts("global object assertions passed");
}
