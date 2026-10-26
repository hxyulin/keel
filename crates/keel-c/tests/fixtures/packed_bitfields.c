struct __attribute__((packed)) Tiny { signed int value:3; };
struct __attribute__((packed)) Cross { unsigned int low:31, high:2; };
struct __attribute__((packed)) Wide {
    unsigned char lead:7;
    unsigned long value:64;
    signed int sign:9;
    unsigned char tail;
};
struct Member { unsigned int low:31 __attribute__((packed)); unsigned int high:2 __attribute__((packed)); };
#pragma pack(push, 2)
struct Pragma { unsigned int low:31, high:2; };
#pragma pack(pop)
union __attribute__((packed)) Choice { signed int sign:9; unsigned char bytes[2]; };
struct __attribute__((packed)) Zero { unsigned int low:3; unsigned int :0; unsigned int high:3; };
_Static_assert(sizeof(struct Tiny) == 1, "tiny");
_Static_assert(sizeof(struct Cross) == 5, "cross");
_Static_assert(sizeof(struct Wide) == 11, "wide");
_Static_assert(__builtin_offsetof(struct Wide, tail) == 10, "tail");
_Static_assert(sizeof(struct Member) == 5, "member");
_Static_assert(sizeof(struct Pragma) == 6 && _Alignof(struct Pragma) == 2, "pragma");
_Static_assert(sizeof(union Choice) == 2, "union");
_Static_assert(sizeof(struct Zero) == 5, "zero width");

static struct Wide initialized = { 85, 0xfedcba9876543210UL, -129, 53 };

unsigned long packed_read(void *pointer, unsigned which) {
    switch (which) {
    case 0: return ((struct Tiny *)pointer)->value;
    case 1: return ((struct Cross *)pointer)->high;
    case 2: return ((struct Wide *)pointer)->value;
    case 3: return ((struct Wide *)pointer)->sign;
    case 4: return ((struct Member *)pointer)->high;
    case 5: return ((struct Pragma *)pointer)->high;
    case 6: return ((union Choice *)pointer)->sign;
    default: return ((struct Zero *)pointer)->high;
    }
}
void packed_write(void *pointer, unsigned which, unsigned long value) {
    switch (which) {
    case 0: ((struct Tiny *)pointer)->value = value; break;
    case 1: ((struct Cross *)pointer)->high = value; break;
    case 2: ((struct Wide *)pointer)->value = value; break;
    case 3: ((struct Wide *)pointer)->sign = value; break;
    case 4: ((struct Member *)pointer)->high = value; break;
    case 5: ((struct Pragma *)pointer)->high = value; break;
    case 6: ((union Choice *)pointer)->sign = value; break;
    default: ((struct Zero *)pointer)->high = value; break;
    }
}
void packed_initial(unsigned char *out, int local) {
    struct Wide temporary = { 85, 0xfedcba9876543210UL, -129, 53 };
    unsigned char *bytes = (unsigned char *)(local ? &temporary : &initialized);
    for (int i = 0; i < 11; i++) out[i] = bytes[i];
}
long packed_update(void *pointer, int value) {
    volatile struct Wide *p = pointer;
    p->sign += value;
    return ++p->sign;
}
