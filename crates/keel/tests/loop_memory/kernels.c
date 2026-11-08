typedef unsigned char uint8_t;
typedef unsigned short uint16_t;
typedef unsigned int uint32_t;
typedef __UINT64_TYPE__ uint64_t;
typedef __SIZE_TYPE__ size_t;
#define UINT64_C(n) n##ULL
void *memcpy(void *, const void *, size_t);
static uint64_t read64(const uint8_t *p) { uint64_t v; memcpy(&v,p,8); return v; }
static uint32_t read32(const uint8_t *p) { uint32_t v; memcpy(&v,p,4); return v; }
static uint16_t read16(const uint8_t *p) { uint16_t v; memcpy(&v,p,2); return v; }
/* Same bounded wide-compare / first-difference recipe used by ZSTD_count
   and LZ4_count, expressed with byte counts so even a zero-size input is valid. */
size_t probe_prefix(const uint8_t *a,const uint8_t *b,size_t n) {
    size_t i=0;
    if(n>=8) {
        uint64_t diff=read64(a)^read64(b);
        if(diff)return (size_t)__builtin_ctzll(diff)/8;
        i=8;
        while(n-i>=8) {
            diff=read64(a+i)^read64(b+i);
            if(diff)return i+(size_t)__builtin_ctzll(diff)/8;
            i+=8;
        }
    }
    if(n-i>=4&&read32(a+i)==read32(b+i))i+=4;
    if(n-i>=2&&read16(a+i)==read16(b+i))i+=2;
    if(n-i&&a[i]==b[i])i++;
    return i;
}

/* These are the scalar memory recurrences in yyjson's validation visitor.
   Input bytes may alias hash. Every iteration's store must remain observable
   to the next byte read. Word input is already passed by value. */
void hash_word(uint64_t *hash, uint64_t value) {
    for (unsigned i=0; i<8; i++) {
        *hash=(*hash^(uint8_t)value)*UINT64_C(1099511628211);
        value>>=8;
    }
}
void hash_bytes(uint64_t *hash, const uint8_t *s, size_t n) {
    for (size_t i=0; i<n; i++)
        *hash=(*hash^s[i])*UINT64_C(1099511628211);
}
