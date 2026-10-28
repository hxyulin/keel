"""Normalized Mach-O (arm64) and ELF64 (x86-64) object structure for assembler A/B.

Sections are keyed by name (ELF: name plus COMDAT signature and occurrence) so
section ordering and addresses can differ. Relocations are section-relative and
local targets resolve to (section, offset) so label naming cannot hide or fake a
difference. Raw bytes are compared separately from relocation records.
"""
import re
import struct

MACHO_MAGIC = 0xfeedfacf
ELF_MAGIC = b'\x7fELF'

UNWIND_SECTIONS = ('__TEXT,__eh_frame', '__LD,__compact_unwind', '__TEXT,__gcc_except_tab',
                   '.eh_frame', '.gcc_except_table')


def section_class(name):
    """Group sections so EH/debug differences are reported apart from code/data."""
    if name.startswith('__DWARF,') or name.startswith('.debug_') or name.startswith('.rela.debug_'):
        return 'debug'
    base = name.split('#')[0]
    if base in UNWIND_SECTIONS or base.startswith('.gcc_except_table') or base in ('.rela.eh_frame',):
        return 'unwind'
    if base in ('.comment', '.llvm_addrsig', '.note.GNU-stack'):
        return 'metadata'
    return 'code_data'


def parse(path):
    data = path.read_bytes()
    if len(data) >= 4 and struct.unpack_from('<I', data)[0] == MACHO_MAGIC:
        return macho(data)
    if data[:4] == ELF_MAGIC:
        return elf(data)
    raise ValueError(f'{path}: not a 64-bit Mach-O or ELF object')


def macho(data):
    u32 = lambda at: struct.unpack_from('<I', data, at)[0]
    cstr = lambda at: data[at:at + 16].split(b'\0')[0].decode()
    sections, build, hints, symtab = [], None, None, None
    at = 32
    cputype = u32(4)
    if cputype != 0x100000c:
        raise ValueError('Mach-O normalization supports arm64 objects only')
    for _ in range(u32(16)):
        cmd, size = u32(at), u32(at + 4)
        if cmd == 0x19:
            for i in range(u32(at + 64)):
                pos = at + 72 + i * 80
                addr, length = struct.unpack_from('<QQ', data, pos + 32)
                start, align, reloc, count, flags = struct.unpack_from('<IIIII', data, pos + 48)
                zerofill = flags & 255 in (1, 12, 18)
                sections.append({'name': cstr(pos + 16) + ',' + cstr(pos), 'address': addr, 'size': length,
                                 'align': 1 << align, 'type': flags & 255, 'attributes': flags & ~255,
                                 'data': b'' if zerofill else data[start:start + length],
                                 'reloc_at': reloc, 'reloc_count': count})
        elif cmd == 0x32:
            build = list(struct.unpack_from('<III', data, at + 8))
        elif cmd == 0x2e:
            offset, length = u32(at + 8), u32(at + 12)
            hints = data[offset:offset + length].hex()
        elif cmd == 2:
            symtab = struct.unpack_from('<IIII', data, at + 8)
        at += size
    raw_symbols, symbols = [], []
    if symtab:
        offset, count, strings, _ = symtab
        for i in range(count):
            string, kind, sect, desc, value = struct.unpack_from('<IBBHQ', data, offset + i * 16)
            end = data.index(b'\0', strings + string)
            name = data[strings + string:end].decode(errors='replace')
            raw_symbols.append((name, kind, sect, desc, value))
            where = ['absolute', value] if kind & 0xe == 2 else (['common-size', value] if value else None)
            if kind & 0xe == 0xa:  # N_INDR stores the aliased symbol's string-table offset.
                end_alias = data.index(b'\0', strings + value)
                where = ['indirect', data[strings + value:end_alias].decode(errors='replace')]
            if sect:
                s = sections[sect - 1]
                where = [s['name'], value - s['address']]
            symbols.append([name, kind, desc, where])
    for section in sections:
        records = []
        for i in range(section['reloc_count']):
            address, info = struct.unpack_from('<iI', data, section['reloc_at'] + i * 8)
            index, typ = info & 0xffffff, info >> 28
            pcrel, length, external = (info >> 24) & 1, (info >> 25) & 3, (info >> 27) & 1
            if typ == 10:  # ARM64_RELOC_ADDEND: low bits carry the addend, not a symbol.
                target = ['addend', (index ^ 0x800000) - 0x800000]
            elif external:
                name, kind, ordinal, _, value = raw_symbols[index]
                target = (['defined', sections[ordinal - 1]['name'], value - sections[ordinal - 1]['address']]
                          if ordinal and not kind & 1 else ['external', name])
            else:
                target = ['section', sections[index - 1]['name']]
            records.append([address, typ, pcrel, length, target])
        section['relocations'] = records
    # Section-relative (non-extern) ARM64_RELOC_UNSIGNED fields embed the target section's
    # object-local address; subtract it so section placement order cannot masquerade as a byte diff.
    for section in sections:
        data_norm = bytearray(section['data'])
        for address, typ, _, length, target in section['relocations']:
            if typ == 0 and target[0] == 'section' and length in (2, 3) and section['data']:
                width = 4 if length == 2 else 8
                fmt = '<I' if width == 4 else '<Q'
                base = next(s['address'] for s in sections if s['name'] == target[1])
                value = struct.unpack_from(fmt, data_norm, address)[0]
                struct.pack_into(fmt, data_norm, address, (value - base) % (1 << (8 * width)))
        section['data_norm'] = bytes(data_norm)
    out = {}
    for s in sections:
        if s['name'] in out:
            raise ValueError('duplicate Mach-O section names are unsupported by normalization')
        out[s['name']] = {k: s[k] for k in ('size', 'align', 'type', 'attributes', 'data', 'data_norm', 'relocations')}
    return {'format': 'macho', 'cputype': cputype, 'header': [u32(8), u32(12), u32(24)],
            'sections': out, 'symbols': sorted(symbols, key=repr),
            'build_version': build, 'linker_hints': hints}


def elf(data):
    if data[4:6] != b'\x02\x01':
        raise ValueError('ELF normalization requires little-endian ELF64')
    shoff, = struct.unpack_from('<Q', data, 0x28)
    shentsize, shnum, shstrndx = struct.unpack_from('<HHH', data, 0x3a)
    headers = []
    for i in range(shnum):
        name, typ, flags, addr, offset, size, link, info, align, entsize = struct.unpack_from(
            '<IIQQQQIIQQ', data, shoff + i * shentsize)
        headers.append(dict(name_at=name, type=typ, flags=flags, offset=offset, size=size, link=link, info=info,
                            align=align, entsize=entsize))
    strtab = headers[shstrndx]

    def string(table, at):
        start = table['offset'] + at
        return data[start:data.index(b'\0', start)].decode(errors='replace')

    for h in headers:
        h['name'] = string(strtab, h['name_at'])
    symtab = next((h for h in headers if h['type'] == 2), None)
    raw_symbols = []
    if symtab:
        names = headers[symtab['link']]
        for i in range(symtab['size'] // 24):
            name, info, other, shndx, value, size = struct.unpack_from('<IBBHQQ', data, symtab['offset'] + i * 24)
            raw_symbols.append(dict(name=string(names, name), bind=info >> 4, type=info & 15, vis=other & 3,
                                    shndx=shndx, value=value, size=size))
    # COMDAT groups: signature symbol names member sections.
    group_of = {}
    for h in headers:
        if h['type'] == 17:
            words = struct.unpack_from(f'<{h["size"] // 4}I', data, h['offset'])
            signature = raw_symbols[h['info']]['name'] if h['info'] < len(raw_symbols) else '?'
            h['group_signature'] = signature
            h['group_flags'] = words[0]
            for member in words[1:]:
                group_of[member] = signature
    keys, seen = [], {}
    for i, h in enumerate(headers):
        base = h['name'] + (f'[{group_of[i]}]' if i in group_of else '')
        if h['type'] == 17:
            base = f'.group[{h["group_signature"]}]'
        n = seen.get(base, 0)
        seen[base] = n + 1
        keys.append(base if n == 0 else f'{base}#{n}')

    def symbol_name(sym):
        if sym['type'] == 3 and sym['shndx'] < len(keys):
            return 'section:' + keys[sym['shndx']]
        return sym['name']

    def where(sym):
        if sym['shndx'] == 0:
            return None
        if sym['shndx'] >= 0xff00:
            return ['special', sym['shndx'], sym['value']]
        return [keys[sym['shndx']], sym['value']]

    symbols = [[symbol_name(s), s['bind'], s['type'], s['vis'], where(s), s['size']] for s in raw_symbols[1:]]
    sections = {}
    for i, h in enumerate(headers):
        if i == 0 or h['type'] in (2, 3, 4, 9):  # symtab/strtab/rela/rel are compared via normalized records.
            continue
        if h['type'] == 17:
            members = struct.unpack_from(f'<{h["size"] // 4}I', data, h['offset'])[1:]
            content = repr([h['group_flags'], sorted(keys[m] for m in members)]).encode()
        else:
            content = b'' if h['type'] == 8 else data[h['offset']:h['offset'] + h['size']]
        sections[keys[i]] = dict(size=h['size'], align=h['align'], type=h['type'], attributes=h['flags'],
                                 entsize=h['entsize'], data=content, relocations=[])
        if h['type'] == 0x6fff4c03:  # SHT_LLVM_ADDRSIG: ULEB symbol indexes, not arbitrary bytes.
            if symtab is None or h['link'] >= len(headers) or headers[h['link']] is not symtab:
                raise ValueError('ELF address-significance section must link to the symbol table')
            indexes, value, shift = [], 0, 0
            for byte in content:
                value |= (byte & 0x7f) << shift
                if byte & 0x80:
                    shift += 7
                else:
                    if not 0 < value < len(raw_symbols):
                        raise ValueError('invalid ELF address-significance symbol index')
                    indexes.append(symbols[value - 1])
                    value, shift = 0, 0
            if shift:
                raise ValueError('unterminated ELF address-significance ULEB')
            sections[keys[i]]['data_norm'] = repr(sorted(indexes, key=repr)).encode()
    for h in headers:
        if h['type'] == 9:
            raise ValueError('ELF SHT_REL normalization is unsupported (expected x86-64 RELA)')
        if h['type'] != 4:
            continue
        if symtab is None or h['link'] >= len(headers) or headers[h['link']] is not symtab:
            raise ValueError('ELF relocation section must link to the symbol table')
        target = keys[h['info']]
        records = []
        for i in range(h['size'] // 24):
            offset, info, addend = struct.unpack_from('<QQq', data, h['offset'] + i * 24)
            sym = raw_symbols[info >> 32] if info >> 32 else None
            if sym is None:
                to = ['absolute', addend]
            elif sym['type'] == 3 or (sym['bind'] == 0 and 0 < sym['shndx'] < 0xff00):
                to = ['section', keys[sym['shndx']], sym['value'] + addend]
            else:
                to = ['symbol', sym['name'], addend]
            records.append([offset, info & 0xffffffff, to])
        if target in sections:
            sections[target]['relocations'] = sorted(records, key=repr)
    return {'format': 'elf', 'cputype': struct.unpack_from('<H', data, 0x12)[0],
            'header': [data[4:9].hex(), *struct.unpack_from('<HHI', data, 0x10),
                       struct.unpack_from('<I', data, 0x30)[0]], 'sections': sections,
            'symbols': sorted(symbols, key=repr), 'build_version': None, 'linker_hints': None}


def compare(ref, got):
    """Return a JSON-friendly comparison; never treats assembly success as equality."""
    out = {'sections': {}, 'missing_sections': [], 'extra_sections': [],
           'format_equal': ref['format'] == got['format'], 'cputype_equal': ref['cputype'] == got['cputype'],
           'header_equal': ref.get('header') == got.get('header')}

    def relocations(obj, section):
        # ARM64 ADDEND/SUBTRACTOR records and their successor form ordered pairs.
        # Sort independent relocation groups, preserving the pairing and multiplicity.
        records = iter(section['relocations'])
        groups = []
        for record in records:
            group = [record]
            if obj['format'] == 'macho' and record[1] in (1, 10):
                group.append(next(records, ['missing-paired-relocation']))
            groups.append(group)
        return sorted(map(repr, groups))
    for name in ref['sections']:
        if name not in got['sections']:
            out['missing_sections'].append(name)
    for name in got['sections']:
        if name not in ref['sections']:
            out['extra_sections'].append(name)
    classes = {}
    for name, a in ref['sections'].items():
        b = got['sections'].get(name)
        cls = section_class(name)
        row = {'class': cls}
        if b is None:
            row.update(present=False)
        else:
            row.update(present=True, raw_bytes=a['data'] == b['data'],
                       bytes=a.get('data_norm', a['data']) == b.get('data_norm', b['data']), size=a['size'] == b['size'],
                       type_flags=(a['type'], a['attributes']) == (b['type'], b['attributes']),
                       align_ok=b['align'] >= a['align'], align_equal=b['align'] == a['align'],
                       entsize=a.get('entsize') == b.get('entsize'),
                       relocations=relocations(ref, a) == relocations(got, b),
                       reloc_order=a['relocations'] == b['relocations'])
            if not row['bytes'] and a['data'] and b['data']:
                n = min(len(a['data']), len(b['data']))
                row['first_byte_diff'] = next((i for i in range(n) if a['data'][i] != b['data'][i]), n)
            if not row['relocations']:
                ra, rb = a['relocations'], b['relocations']
                row['reloc_counts'] = [len(ra), len(rb)]
                diff_a = [r for r in ra if r not in rb][:3]
                diff_b = [r for r in rb if r not in ra][:3]
                row['reloc_examples'] = {'ref_only': diff_a, 'keel_only': diff_b}
        out['sections'][name] = row
        ok = row.get('present') and all(row[k] for k in ('bytes', 'size', 'type_flags', 'align_equal', 'entsize', 'relocations'))
        classes.setdefault(cls, []).append(bool(ok))
    for name in out['extra_sections']:
        classes.setdefault(section_class(name), []).append(False)
    out['class_equal'] = {cls: all(v) for cls, v in classes.items()}
    sa, sb = ref['symbols'], got['symbols']
    out['symbols_equal'] = sa == sb
    if not out['symbols_equal']:
        out['symbol_counts'] = [len(sa), len(sb)]
        out['symbol_examples'] = {'ref_only': [s for s in sa if s not in sb][:5],
                                  'keel_only': [s for s in sb if s not in sa][:5]}
    # Benign tier (documented, never folded into the strict verdict): Mach-O assembler-temp `ltmp*`
    # locals and the S_ATTR_LOC_RELOC/S_ATTR_EXT_RELOC section bits, which MH_OBJECT linkers recompute.
    if ref['format'] == got['format'] == 'macho':
        temp = lambda syms: [x for x in syms if not (
            re.fullmatch(r'ltmp(?:\d+|_Lkeel_metadata_section\d+)', x[0]) and x[1:3] == [14, 0])]
        out['symbols_equal_benign'] = temp(sa) == temp(sb)
        for name, row in out['sections'].items():
            b = got['sections'].get(name)
            if b is not None:
                a = ref['sections'][name]
                row['type_flags_benign'] = (a['type'], a['attributes'] & ~0x300) == (b['type'], b['attributes'] & ~0x300)
    else:
        out['symbols_equal_benign'] = out['symbols_equal']
        for row in out['sections'].values():
            if row.get('present'):
                row['type_flags_benign'] = row['type_flags']
    benign = lambda row: row.get('present') and all(row.get(k) for k in ('bytes', 'size', 'type_flags_benign', 'align_equal', 'entsize', 'relocations'))
    benign_classes = {}
    for name, row in out['sections'].items():
        benign_classes.setdefault(row['class'], []).append(bool(benign(row)))
    for name in out['extra_sections']:
        benign_classes.setdefault(section_class(name), []).append(False)
    out['class_equal_benign'] = {cls: all(v) for cls, v in benign_classes.items()}
    out['build_version_equal'] = ref['build_version'] == got['build_version']
    out['linker_hints_equal'] = ref['linker_hints'] == got['linker_hints']
    same_object_kind = out['format_equal'] and out['cputype_equal'] and out['header_equal']
    out['code_data_equal'] = same_object_kind and out['class_equal_benign'].get('code_data', True) and out['symbols_equal_benign']
    out['all_equal'] = all(out['class_equal'].values()) and out['symbols_equal'] and out['build_version_equal'] \
        and out['linker_hints_equal'] and not out['missing_sections'] and not out['extra_sections'] and same_object_kind
    out['all_equal_benign'] = all(out['class_equal_benign'].values()) and out['symbols_equal_benign'] \
        and out['build_version_equal'] and out['linker_hints_equal'] and not out['missing_sections'] and same_object_kind
    return out
