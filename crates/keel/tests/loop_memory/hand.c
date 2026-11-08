#include <stdint.h>
#include <stddef.h>
#include <string.h>
static uint64_t read64(const uint8_t *p) { uint64_t v; memcpy(&v,p,8); return v; }
static uint32_t read32(const uint8_t *p) { uint32_t v; memcpy(&v,p,4); return v; }
static uint16_t read16(const uint8_t *p) { uint16_t v; memcpy(&v,p,2); return v; }
/* Same bounded wide-compare / first-difference recipe used by ZSTD_count
   and LZ4_count, expressed with byte counts so even a zero-size input is valid. */
size_t hand_prefix(const uint8_t *a,const uint8_t *b,size_t n) {
    size_t i=0, whole=n&~(size_t)7;
    if (whole) do {
        uint64_t diff=read64(a+i)^read64(b+i);
        if(diff)return i+(size_t)__builtin_ctzll(diff)/8;
        i+=8;
    } while(i<whole);
    if(n-i>=4&&read32(a+i)==read32(b+i))i+=4;
    if(n-i>=2&&read16(a+i)==read16(b+i))i+=2;
    if(n-i&&a[i]==b[i])i++;
    return i;
}

/* Portable handwritten comparison. Retaining each byte-loop store preserves
   self-aliasing inputs; word has no intervening access to observe its stores. */
void hand_word(uint64_t *hash,uint64_t value) {
    uint64_t h=*hash;
    h=(h^(uint8_t)value)*UINT64_C(1099511628211); value>>=8;
    h=(h^(uint8_t)value)*UINT64_C(1099511628211); value>>=8;
    h=(h^(uint8_t)value)*UINT64_C(1099511628211); value>>=8;
    h=(h^(uint8_t)value)*UINT64_C(1099511628211); value>>=8;
    h=(h^(uint8_t)value)*UINT64_C(1099511628211); value>>=8;
    h=(h^(uint8_t)value)*UINT64_C(1099511628211); value>>=8;
    h=(h^(uint8_t)value)*UINT64_C(1099511628211); value>>=8;
    h=(h^(uint8_t)value)*UINT64_C(1099511628211); value>>=8;
    *hash=h;
}
void hand_bytes(uint64_t *hash,const uint8_t *s,size_t n) {
    if (!n) return;
    uint64_t h=*hash;
    for (size_t i=0;i<n;i++) { h=(h^s[i])*UINT64_C(1099511628211); *hash=h; }
}
