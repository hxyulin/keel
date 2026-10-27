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

S1 aggregate1(S1 v) { asm("xorb $90,%0" : "+q"(v)); return v; }
S2 aggregate2(S2 v) { asm("xorw $23130,%0" : "+r"(v)); return v; }
S4 aggregate4(S4 v) { asm("xorl $1515870810,%0" : "+r"(v)); return v; }
S8 aggregate8(S8 v) { S8 mask={0xa55aa55aa55aa55aUL}; asm("xorq %1,%0" : "+r"(v) : "r"(mask)); return v; }
U64 aggregate_float(Float8 v) { U64 r; asm("movq %1,%0" : "=r"(r) : "r"(v)); return r; }
U64 aggregate_union(Union8 v) { U64 r; asm("movq %1,%0" : "=r"(r) : "r"(v)); return r; }
U64 aggregate_padding(const struct Padded *v) { U64 r; asm("movq %1,%0" : "=r"(r) : "g"(*v)); return r; }
void aggregate_packed(struct Packed *v) { asm("addq $7,%0" : "+r"(v->value)); }
void aggregate_volatile(struct Volatile *v) { asm("addq $11,%0" : "+r"(*v)); }
S8 aggregate_tied(S8 v) { S8 out; asm("xorq $17,%0" : "=r"(out) : "0"(v)); return out; }
S8 aggregate_fixed(S8 v) { asm("addq $13,%0" : "+a"(v)); return v; }
int aggregate_evaluations;
static __attribute__((noinline)) S8 make_record(U64 n) { ++aggregate_evaluations; S8 v={n}; return v; }
static __attribute__((noinline)) S8 *record_location(S8 *p) { ++aggregate_evaluations; return p; }
void aggregate_once(S8 *p, U64 n) { asm("addq %[in],%[out]" : [out] "+r"(*record_location(p)) : [in] "r"(make_record(n))); }
void aggregate_odd_memory(struct Odd *p) { asm("movb $73,%0" : "=g"(*p)); }
U64 aggregate_mixed(S8 *p, U64 *memory, U64 n) {
    U64 result;
    asm("addq %[in],%[word]; movq %[word],%[slot]; movq %[word],%[result]"
        : [slot] "=m"(*memory), [word] "+r"(*p), [result] "=r"(result)
        : [in] "r"((S8){n}));
    return result;
}

S8 aggregate_generic(S8 v) { asm("addq $3,%0" : "+g"(v)); return v; }

/* The kernel's WRITE_ONCE branches are type-checked even for an 8-byte wrapper. */
void aggregate_write_once(S8 *destination, S8 value) {
    switch (sizeof(value)) {
    case 1: asm volatile("movb %b1,%0" : "=m"(*destination) : "q"(value) : "memory"); break;
    case 2: asm volatile("movw %w1,%0" : "=m"(*destination) : "r"(value) : "memory"); break;
    case 4: asm volatile("movl %k1,%0" : "=m"(*destination) : "r"(value) : "memory"); break;
    case 8: asm volatile("movq %q1,%0" : "=m"(*destination) : "r"(value) : "memory"); break;
    }
}

U64 aggregate_volatile_input(const struct Volatile *p) {
    U64 bits; asm("movq %1,%0" : "=r"(bits) : "r"(*p)); return bits;
}
