/* CRC32C updates require SSE4.2 and consume the accumulator's low 32 bits. */
#ifndef _KEEL_CRC32INTRIN_H
#define _KEEL_CRC32INTRIN_H
#ifndef __x86_64__
#error "Keel x86 intrinsic headers require x86-64"
#endif
static __inline__ __attribute__((always_inline)) unsigned int _mm_crc32_u8(unsigned int crc, unsigned char value) {
  return (unsigned int)__builtin_keel_string_sse42crc32b((int)crc,(signed char)value);
}
static __inline__ __attribute__((always_inline)) unsigned int _mm_crc32_u16(unsigned int crc, unsigned short value) {
  return (unsigned int)__builtin_keel_string_sse42crc32w((int)crc,(short)value);
}
static __inline__ __attribute__((always_inline)) unsigned int _mm_crc32_u32(unsigned int crc, unsigned int value) {
  return (unsigned int)__builtin_keel_string_sse42crc32d((int)crc,(int)value);
}
static __inline__ __attribute__((always_inline)) unsigned long long _mm_crc32_u64(unsigned long long crc, unsigned long long value) {
  return (unsigned long long)(unsigned int)__builtin_keel_string_sse42crc32q((int)crc,(long long)value);
}
#endif /* _KEEL_CRC32INTRIN_H */
