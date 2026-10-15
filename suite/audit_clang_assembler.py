#!/usr/bin/env python3
"""Compile unchanged C/Clang assembly and audit Keel against Clang Mach-O output.

Example: --input-dir crates/keel-c/tests/programs --input-dir /path/to/lua/pre
No fixture executes. __eh_frame/compact-unwind encodings can differ legally;
compare their metadata in focused Rust tests instead of comparing raw bytes here.
"""
import argparse
import hashlib
import json
from pathlib import Path
import struct
import subprocess


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def macho(path):
    data = path.read_bytes()
    u32 = lambda at: struct.unpack_from('<I', data, at)[0]
    name = lambda at: data[at:at + 16].split(b'\0')[0].decode()
    sections, build, hints, symtab = [], None, None, None
    at = 32
    for _ in range(u32(16)):
        cmd, size = u32(at), u32(at + 4)
        if cmd == 0x19:
            for i in range(u32(at + 64)):
                pos = at + 72 + i * 80
                length, start, align, reloc, count, flags = struct.unpack_from('<QIIIII', data, pos + 40)
                zero = flags & 255 in (1, 12, 18)
                sections.append({'name': name(pos + 16) + ',' + name(pos),
                                 'address': struct.unpack_from('<Q', data, pos + 32)[0],
                                 'size': length, 'alignment': align, 'type': flags & 255,
                                 'data': b'' if zero else data[start:start + length],
                                 'reloc': reloc, 'count': count})
        elif cmd == 0x32:
            build = list(struct.unpack_from('<III', data, at + 8))
        elif cmd == 0x2e:
            offset, length = u32(at + 8), u32(at + 12)
            hints = data[offset:offset + length].hex()
        elif cmd == 2:
            symtab = struct.unpack_from('<IIII', data, at + 8)
        at += size
    symbols = []
    offset, count, strings, _ = symtab
    for i in range(count):
        string, kind, section, desc, value = struct.unpack_from('<IBBHQ', data, offset + i * 16)
        end = data.index(b'\0', strings + string)
        symbols.append((data[strings + string:end].decode(), kind, section, desc, value))
    for section in sections:
        records = []
        for i in range(section['count']):
            address, info = struct.unpack_from('<II', data, section['reloc'] + i * 8)
            index = info & 0xffffff
            typ = info >> 28
            if typ == 10:  # ARM64_RELOC_ADDEND's low bits are not a symbol index.
                target = ['addend', (index ^ 0x800000) - 0x800000]
            elif info & (1 << 27):
                symbol, _, ordinal, _, value = symbols[index]
                target = ['defined', sections[ordinal - 1]['name'], value - sections[ordinal - 1]['address']] if ordinal else ['external', symbol]
            else:
                target = ['section', sections[index - 1]['name']]
            records.append([address, typ, (info >> 24) & 1, (info >> 25) & 3, target])
        section['relocations'] = sorted(records)
    return sections, build, hints


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--input-dir', type=Path, action='append', required=True)
    parser.add_argument('--compiler', type=Path, default=Path('target/release/keel'))
    parser.add_argument('--work-dir', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    args.work_dir.mkdir(parents=True, exist_ok=True)
    rows = []
    for directory in args.input_dir:
        for source in sorted(directory.iterdir()):
            if source.suffix not in ('.c', '.i'):
                continue
            assembly = args.work_dir / f'{len(rows)}-{source.stem}.s'
            ref, actual = assembly.with_suffix('.clang.o'), assembly.with_suffix('.keel.o')
            commands = [
                ['clang', '-S', '-O2', '-target', 'arm64-apple-macosx11.0.0', '-mcpu=apple-m5', str(source), '-o', str(assembly)],
                ['clang', '-c', '-target', 'arm64-apple-macosx11.0.0', '-mcpu=apple-m5', str(assembly), '-o', str(ref)],
                [str(args.compiler), 'asm', '--cpu', 'apple-m5', str(assembly), '-o', str(actual)],
            ]
            row = {'input': str(source), 'input_sha256': sha(source), 'commands': commands}
            rows.append(row)
            for command in commands:
                result = subprocess.run(command, capture_output=True, text=True)
                if result.returncode:
                    row['error'] = {'command': command, 'stderr': result.stderr}
                    break
            else:
                row['assembly_sha256'] = sha(assembly)
                row['keel_object_sha256'], row['clang_object_sha256'] = sha(actual), sha(ref)
                a, ab, ah = macho(actual)
                b, bb, bh = macho(ref)
                row['build_version_match'] = ab == bb
                row['linker_hints_match'] = ah == bh
                row['sections'] = []
                for expected in b:
                    if expected['name'] in ('__TEXT,__eh_frame', '__LD,__compact_unwind') or expected['name'].startswith('__DWARF,'):
                        continue
                    found = next((s for s in a if s['name'] == expected['name']), None)
                    row['sections'].append({'name': expected['name'], 'bytes_match': found is not None and found['data'] == expected['data'],
                                            'type_size_match': found is not None and (found['type'], found['size']) == (expected['type'], expected['size']),
                                            'alignment_sufficient': found is not None and found['alignment'] >= expected['alignment'],
                                            'relocations_match': found is not None and found['relocations'] == expected['relocations']})
                row['ok'] = row['build_version_match'] and row['linker_hints_match'] and all(all(v for k, v in s.items() if k != 'name') for s in row['sections'])
            print(source.name, row.get('ok', False), row.get('error', ''))
    report = {'clang': subprocess.check_output(['clang', '--version'], text=True), 'keel_sha256': sha(args.compiler), 'inputs': rows,
              'passed': sum(r.get('ok', False) for r in rows), 'total': len(rows)}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + '\n')
    raise SystemExit(0 if report['passed'] == report['total'] else 1)


if __name__ == '__main__':
    main()
