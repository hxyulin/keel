#include <stdint.h>
#include <stdio.h>
#include <string.h>
typedef unsigned long U64;
typedef struct { unsigned char x; } S1;
typedef struct { unsigned short x; } S2;
typedef struct { unsigned int x; } S4;
typedef struct { U64 x; } S8;
typedef struct { double x; } Float8;
typedef union { double f; U64 bits; } Union8;
struct Padded { unsigned char a; unsigned int b; };
struct Packed { unsigned char before; S8 value; unsigned char after; } __attribute__((packed));
struct Volatile { volatile U64 x; };
struct Odd { unsigned char bytes[3]; };
extern S1 aggregate1(S1); extern S2 aggregate2(S2); extern S4 aggregate4(S4); extern S8 aggregate8(S8);
extern U64 aggregate_float(Float8),aggregate_union(Union8),aggregate_padding(const struct Padded *);
extern void aggregate_packed(struct Packed *),aggregate_volatile(struct Volatile *),aggregate_once(S8 *,U64),aggregate_odd_memory(struct Odd *);
extern S8 aggregate_tied(S8),aggregate_fixed(S8),aggregate_generic(S8);
extern U64 aggregate_mixed(S8 *,U64 *,U64);
extern void aggregate_write_once(S8 *,S8);
extern U64 aggregate_volatile_input(const struct Volatile *);
extern int aggregate_evaluations;
int main(void) {
    for(U64 n=0;n<256;++n) {
        U64 wide=UINT64_C(0xfedcba9876543210)^n;
        if(aggregate1((S1){n}).x!=(unsigned char)(n^90))return 1;
        if(aggregate2((S2){n*257}).x!=(unsigned short)((n*257)^23130))return 2;
        if(aggregate4((S4){n*16777217u}).x!=((unsigned)(n*16777217u)^1515870810u))return 3;
        if(aggregate8((S8){wide}).x!=(wide^UINT64_C(0xa55aa55aa55aa55a)))return 4;
        if(aggregate_tied((S8){wide}).x!=(wide^17))return 5;
        if(aggregate_fixed((S8){wide}).x!=wide+13||aggregate_generic((S8){wide}).x!=wide+3)return 6;
        Union8 u; u.bits=UINT64_C(0xfff8000000000000)|n;
        Float8 f; memcpy(&f.x,&u.bits,8);
        if(aggregate_float(f)!=u.bits||aggregate_union(u)!=u.bits)return 7;
        struct Padded padded; unsigned char *bytes=(unsigned char *)&padded;
        for(unsigned i=0;i<8;++i)bytes[i]=(unsigned char)(n+i*29);
        U64 bits;memcpy(&bits,&padded,8);
        if(aggregate_padding(&padded)!=bits)return 8;
        struct Packed packed={0xa5,{wide},0x5a};aggregate_packed(&packed);
        if(packed.before!=0xa5||packed.after!=0x5a||packed.value.x!=wide+7)return 9;
        struct Volatile v={wide};aggregate_volatile(&v);if(v.x!=wide+11||aggregate_volatile_input(&v)!=wide+11)return 10;
        S8 once={wide};aggregate_evaluations=0;aggregate_once(&once,n);
        if(once.x!=wide+n||aggregate_evaluations!=2)return 11;
        struct {unsigned char before; struct Odd value; unsigned char after;} odd={0xa5,{{1,2,3}},0x5a};aggregate_odd_memory(&odd.value);
        if(odd.value.bytes[0]!=73||odd.before!=0xa5||odd.after!=0x5a)return 12;
        U64 memory=0;S8 mixed={wide};U64 r=aggregate_mixed(&mixed,&memory,n);
        if(r!=wide+n||memory!=r||mixed.x!=r)return 13;
        struct {U64 before; S8 data; U64 after;} once_word={0xa55a,{0},0x5aa5};
        aggregate_write_once(&once_word.data,(S8){wide});
        if(once_word.data.x!=wide||once_word.before!=0xa55a||once_word.after!=0x5aa5)return 14;
    }
    puts("aggregate-asm PASS");return 0;
}
