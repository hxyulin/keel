#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif
struct Record { uint8_t tag; int64_t wide; int16_t tail; };
struct __attribute__((packed)) Packed { uint8_t tag; int32_t value; };
int32_t zeroes[7];
int32_t writable = -31;
extern const uint16_t constants[4];
const uint16_t constants[4] = {0, 1, 32768, 65535};
struct Record record = {0xa5, INT64_C(-72623859790382856), -1234};
struct Packed packed = {0x7e, 0x12345678};
static int32_t local_values[3] = {17, -9, 42};
extern int32_t external_value;
extern int32_t external_function(void);
int32_t target(void) { return 73; }
static int32_t local_target(void) { return 101; }
int32_t (*local_function_pointer)(void) = local_target;
int32_t (*local_function_address(void))(void) { return local_target; }
void replace_writable(int32_t value) { writable = value; }
int32_t read_local(void) { return local_values[1]; }
int32_t *local_address(void) { return local_values; }
int32_t *element_address(void) { return &local_values[2]; }
int32_t *external_address(void) { return &external_value; }
int32_t (*function_address(void))(void) { return target; }
int32_t *data_pointer = &writable;
int32_t *element_pointer = &local_values[2];
int16_t *field_pointer = &record.tail;
int32_t *external_pointer = &external_value;
int32_t (*function_pointer)(void) = target;
int32_t (*external_function_pointer)(void) = external_function;
struct PointerRecord { uint8_t tag; int32_t *ptr; int16_t *field; };
struct PointerRecord pointer_record = {11, &local_values[1], &record.tail};
int32_t *null_pointer = 0;
extern const char text[5];
const char text[5] = {'a', '\0', (char)0xff, '\n', 'z'};
#ifdef __cplusplus
}
#endif
