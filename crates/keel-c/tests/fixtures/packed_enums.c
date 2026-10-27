enum U8 flip8(enum U8 value) { return value ^ 255; }
enum S8 neg8(enum S8 value) { return -value; }
enum U16 flip16(enum U16 value) { return value ^ 65535; }
enum S16 neg16(enum S16 value) { return -value; }
enum U32 flip32(enum U32 value) { return value ^ 0xffffffffU; }
enum U64 flip64(enum U64 value) { return value ^ 0xffffffffffffffffULL; }
long long neg64(enum S64 value) { return -value; }
struct Packet initial_packet = { 0xa1, U8_MAX, -17, U16_MAX, 0xb2 };
int update(struct Packet *packet, volatile enum S8 *out) {
    packet->byte = 254; packet->sign = -125; packet->word = 0x8abc;
    *out = packet->sign;
    return packet->byte + packet->sign + *out;
}
struct Packet packet_roundtrip(struct Packet packet) { packet.byte ^= 255; return packet; }
int enum_promotions(void) {
    enum U8 small = 255; enum S8 negative = -1;
    enum U32 big = U32_MAX;
    enum U8 { INNER = -1 };
    enum U8 shadow = INNER;
    return sizeof(small + 0) == sizeof(int) && small + negative == 254
        && small > -1 && big > -1 == 0 && sizeof(shadow) == sizeof(int);
}
