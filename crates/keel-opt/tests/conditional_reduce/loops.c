#include <stdint.h>
uint32_t conditional32(const int32_t *p,uint64_t n,int32_t cut,uint32_t seed) {
    uint32_t sum=seed;
    for(uint64_t i=0;i<n;i++) sum += p[i]<cut ? (uint32_t)p[i] : 0;
    return sum;
}
uint64_t conditional64(const int64_t *p,uint64_t n,int64_t cut,uint64_t seed) {
    uint64_t sum=seed;
    for(uint64_t i=0;i<n;i++) sum += p[i]<cut ? (uint64_t)p[i] : 0;
    return sum;
}
