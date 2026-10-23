#!/usr/bin/env python3
"""Independent seeded-table oracle; emits the complete 32-byte expected image.

Arguments are harness-owned seed values, never queried/actual results. Example:
  python3 descriptor_expected.py --gdt-base 0x1000 --gdt-limit 0x27 \
      --idt-base 0x2000 --idt-limit 0xfff > /tmp/descriptor-expected.bin
"""
import argparse
import struct
import sys


def expected(gdt_base, gdt_limit, idt_base, idt_limit):
    image = bytearray([0xa5] * 32)
    # Architectural packed little-endian limit:u16, base:u64; no native padding.
    image[1:11] = struct.pack('<HQ', gdt_limit, gdt_base)
    image[17:27] = struct.pack('<HQ', idt_limit, idt_base)
    return bytes(image)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--self-test', action='store_true')
    for table in ('gdt', 'idt'):
        for field in ('base', 'limit'):
            parser.add_argument(f'--{table}-{field}', type=lambda x: int(x, 0))
    args = parser.parse_args()
    if args.self_test:
        assert expected(0x0123456789abcdef, 0x1234,
                        0xfedcba9876543210, 0x5678) == bytes.fromhex(
                            'a5 3412 efcdab8967452301 a5a5a5a5a5a5 '
                            '7856 1032547698badcfe a5a5a5a5a5')
        return
    values = (args.gdt_base, args.gdt_limit, args.idt_base, args.idt_limit)
    for value, bits in zip(values, (64, 16, 64, 16)):
        if value is None or not 0 <= value < (1 << bits):
            parser.error('supply both seeded bases (u64) and limits (u16)')
    sys.stdout.buffer.write(expected(*values))


if __name__ == '__main__':
    main()
