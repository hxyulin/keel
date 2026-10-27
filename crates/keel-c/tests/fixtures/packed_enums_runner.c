extern struct Packet initial_packet;
int main(void) {
    if (flip8(0x81) != 0x7e || neg8(-100) != 100) return 1;
    if (flip16(0x8123) != 0x7edc || neg16(-1234) != 1234) return 2;
    if (flip32(0x81234567U) != 0x7edcba98U) return 3;
    if (flip64(0x8123456789abcdefULL) != 0x7edcba9876543210ULL || neg64(-2147483649LL) != 2147483649LL) return 4;
    if (initial_packet.before != 0xa1 || initial_packet.byte != 255 || initial_packet.sign != -17 || initial_packet.word != 65535 || initial_packet.after != 0xb2) return 5;
    struct Packet p = initial_packet; volatile enum S8 out = 0;
    if (update(&p, &out) != 4 || out != -125 || p.byte != 254 || p.sign != -125 || p.word != 0x8abc || p.before != 0xa1 || p.after != 0xb2) return 6;
    p = packet_roundtrip(p);
    if (p.byte != 1 || p.word != 0x8abc || !enum_promotions()) return 7;
    return 0;
}
