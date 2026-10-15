#!/usr/bin/env python3
"""Inspect trusted generated Mach-O objects independently of keel-link's reader."""
import argparse
from collections import Counter
import json
from pathlib import Path
import struct

RELOCS = ['UNSIGNED', 'SUBTRACTOR', 'BRANCH26', 'PAGE21', 'PAGEOFF12',
          'GOT_LOAD_PAGE21', 'GOT_LOAD_PAGEOFF12', 'POINTER_TO_GOT',
          'TLVP_LOAD_PAGE21', 'TLVP_LOAD_PAGEOFF12', 'ADDEND']
COMMANDS = {0x19: 'LC_SEGMENT_64', 2: 'LC_SYMTAB', 0xb: 'LC_DYSYMTAB',
            0x32: 'LC_BUILD_VERSION', 0x2e: 'LC_LINKER_OPTIMIZATION_HINT'}


def audit(root):
    commands, sections, relocations, symbols, cfi, versions = [Counter() for _ in range(6)]
    objects = sorted(root.glob('*/*.o'))
    undefined = set()
    for path in objects:
        data = path.read_bytes()
        u32 = lambda at: struct.unpack_from('<I', data, at)[0]
        u64 = lambda at: struct.unpack_from('<Q', data, at)[0]
        assert u32(0) == 0xfeedfacf and u32(4) == 0x100000c and u32(12) == 1, path
        at = 32
        for _ in range(u32(16)):
            cmd, length = u32(at), u32(at + 4)
            commands[COMMANDS.get(cmd, hex(cmd))] += 1
            if cmd == 0x19:
                for index in range(u32(at + 64)):
                    s = at + 72 + index * 80
                    name = data[s:s+16].split(b'\0')[0].decode()
                    seg = data[s+16:s+32].split(b'\0')[0].decode()
                    sections[f'{seg},{name}:type={u32(s+64) & 255:#x}'] += 1
                    if name == '__compact_unwind':
                        start = u32(s+48)
                        for entry in range(start, start+u64(s+40), 32):
                            encoding = u32(entry+12)
                            cfi[hex(encoding & 0x0f000000)] += 1
                    for index in range(u32(s+60)):
                        record = u32(s+56)+index*8
                        bits = u32(record+4)
                        kind = bits >> 28
                        relocations[f'{RELOCS[kind] if kind < len(RELOCS) else kind}:width={1 << ((bits>>25)&3)}:external={(bits>>27)&1}:pcrel={(bits>>24)&1}'] += 1
            elif cmd == 2:
                offset, count, strings = u32(at+8), u32(at+12), u32(at+16)
                for i in range(count):
                    s = offset+i*16
                    kind = data[s+4]
                    desc = struct.unpack_from('<H', data, s+6)[0]
                    symbols[f'type={kind:#x}:desc={desc:#x}'] += 1
                    if kind & 0xe == 0 and u64(s+8) == 0:
                        start = strings+u32(s)
                        undefined.add(data[start:data.index(0, start)].decode())
            elif cmd == 0x32:
                versions[f'platform={u32(at+8)}:minimum={u32(at+12):#x}:sdk={u32(at+16):#x}'] += 1
            at += length
    return {'objects': len(objects), 'load_commands': dict(commands),
            'sections': dict(sections), 'relocations': dict(relocations),
            'symbol_forms': dict(symbols), 'compact_unwind_modes': dict(cfi),
            'build_versions': dict(versions), 'undefined_symbols': sorted(undefined)}


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('root', type=Path)
    a = p.parse_args()
    print(json.dumps(audit(a.root), indent=2))

if __name__ == '__main__':
    main()
