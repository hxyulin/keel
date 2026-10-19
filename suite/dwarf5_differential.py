#!/usr/bin/env python3
"""Differential evidence for Keel's AArch64 Mach-O DWARF 5 line tables.

Each C input is compiled once by Clang to assembly (`-gdwarf-5 -S`); that
unchanged text is assembled by both Clang's integrated assembler and
`keel asm`. Per object this records:
- every section other than `__debug_line`: bytes, relocations and flags
  (Keel's extra S_ATTR_EXT_RELOC|S_ATTR_LOC_RELOC bits masked, as in
  assembler_features.py), plus symbols;
- `__debug_line_str` bytes (Keel builds them; equality is expected);
- the decoded line-table header (version, directories, files with directory
  index, MD5 and embedded source) and every decoded row with all flags;
- `dwarfdump --verify` of Keel's object.
`--link` additionally links the Lua objects from both assemblers, runs
dsymutil, verifies both dSYMs and compares their decoded line tables.
The `__debug_line` program bytes themselves are not expected to match:
Keel uses standard opcodes where LLVM uses special opcodes.
"""
import argparse
import concurrent.futures
import json
from pathlib import Path
import re
import subprocess
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from assembler_features import UNWIND, frameless_state, parse_state, unwind  # noqa: E402
from assembler_investigate import inventory  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
TARGET = ['-target', 'arm64-apple-macosx11.0.0', '-mcpu=apple-m5']
RELOC_ATTRS = 0x300
GENERATED = '__DWARF,__debug_line'


def run(cmd, **kw):
    try:
        p = subprocess.run(list(map(str, cmd)), capture_output=True, **kw)
    except OSError as e:
        return {'cmd': list(map(str, cmd)), 'returncode': -1, 'stdout': '', 'stderr': str(e)}
    return {'cmd': list(map(str, cmd)), 'returncode': p.returncode,
            'stdout': p.stdout.decode(errors='replace'), 'stderr': p.stderr.decode(errors='replace')}


def line_tables(path):
    """Every line table: header fields, directories, files and rows."""
    text = run(['dwarfdump', '--debug-line', path])['stdout']
    tables, table, current = [], None, None
    for line in text.splitlines():
        if line.startswith('debug_line['):
            table = {'directories': {}, 'files': {}, 'rows': []}
            tables.append(table)
            current = None
        elif table is None:
            continue
        elif m := re.match(r'\s+(version|address_size|seg_select_size|prologue_length): (\S+)', line):
            table[m[1]] = m[2]
        elif m := re.match(r'include_directories\[\s*(\d+)\] = "(.*)"$', line):
            table['directories'][int(m[1])] = m[2]
        elif m := re.match(r'file_names\[\s*(\d+)\]:', line):
            current = table['files'][int(m[1])] = {}
        elif current is not None and (m := re.match(r'\s+(name|dir_index|md5_checksum|source): (.*)', line)):
            current[m[1]] = m[2]
        elif line.startswith('0x'):
            parts = line.split()
            table['rows'].append([int(parts[0], 16), *map(int, parts[1:7]), sorted(parts[7:])])
    return tables


def resolved(table):
    out = []
    for row in table['rows']:
        f = table['files'][row[3]]
        d = table['directories'][int(f['dir_index'])]
        out.append([row[0], row[1], row[2], f'{d}/{f["name"].strip(chr(34))}', *row[4:]])
    return out


def relative(section, sections):
    """Section bytes with local UNSIGNED 8-byte addends made section-relative.

    The in-place value of a local Mach-O relocation is the target's address in
    the object; `__debug_line` sizes differ, so sections placed after it
    (zero-fill) move. Relocation records themselves are compared exactly.
    """
    data = bytearray(section['data'])
    for address, typ, _pcrel, length, target in section['relocations']:
        if typ == 0 and length == 3 and target[0] == 'section':
            base = sections[target[1]]['address']
            value = int.from_bytes(data[address:address + 8], 'little') - base
            data[address:address + 8] = (value % (1 << 64)).to_bytes(8, 'little')
    return bytes(data)


def named(inv):
    return [x for x in inv['symbols'] if not x['name'].startswith('ltmp')]


def compare_objects(keel, clang):
    a, b = inventory(keel), inventory(clang)
    sa = {s['name']: s for s in a['sections']}
    sb = {s['name']: s for s in b['sections']}
    mismatches, known, unwind_differs = [], [], False
    for name in sorted(sa.keys() | sb.keys()):
        x, y = sa.get(name), sb.get(name)
        if name in UNWIND and (x is None) != (y is None):
            unwind_differs = True
            continue
        if x is None or y is None:
            mismatches.append({'section': name, 'present_keel': x is not None, 'present_clang': y is not None})
            continue
        checks = {'flags': (x['flags'] & ~RELOC_ATTRS) == (y['flags'] & ~RELOC_ATTRS),
                  'alignment': x['alignment'] == y['alignment']}
        if name != GENERATED:
            checks['bytes'] = relative(x, sa) == relative(y, sb)
            checks['relocations'] = x['relocations'] == y['relocations']
        bad = [k for k, v in checks.items() if not v]
        entry = {'section': name, 'differs': bad, 'keel_size': x['size'], 'clang_size': y['size']}
        if name in UNWIND and bad:
            unwind_differs = True
            continue
        if name == '__TEXT,__text' and x['size'] == y['size'] == 0 and set(bad) <= {'flags', 'alignment'}:
            # Same before this change (DWARF 4 too): the writer marks an
            # empty text section with S_ATTR_SOME_INSTRUCTIONS and 4-byte alignment.
            known.append(entry)
        elif bad:
            mismatches.append(entry)
    # Unwind encodings differ before this change too; like
    # assembler_features.py, require semantically equivalent compact entries
    # and identical decoded FDE rows instead of equal bytes.
    unwinding = unwind(keel, clang) if unwind_differs else None
    for entry in (unwinding or {}).get('differing_entries', []):
        # dwarfdump names DWARF registers 72+ B8..; frameless_state uses D8...
        final = entry['keel_fde_final_row']
        if not entry['equivalent'] and final and re.fullmatch(r'0x30*', entry['keel_encoding']) \
                and parse_state(re.sub(r'\bB(\d+)=', r'D\1=', final)) == frameless_state(int(entry['clang_encoding'], 16)):
            entry['equivalent'] = entry['equivalent_after_d_register_naming'] = True
    if unwinding and 'differing_entries' in unwinding:
        unwinding['differences_equivalent'] = all(e['equivalent'] for e in unwinding['differing_entries'])
    if unwinding is not None and not (unwinding.get('same_entry_count') and unwinding.get('fde_per_dwarf_mode_entry')
                                      and unwinding.get('differences_equivalent')
                                      and unwinding.get('remaining_fdes_equal')):
        mismatches.append({'section': 'unwind', 'semantic': unwinding})
    elif unwinding is not None:
        known.append({'section': 'unwind', 'semantic_equivalent': True,
                      'differing_compact_entries': len(unwinding.get('differing_entries', []))})
    kt, ct = line_tables(keel), line_tables(clang)
    headers_equal = [{k: v for k, v in t.items() if k not in ('rows', 'prologue_length')} for t in kt] == \
        [{k: v for k, v in t.items() if k not in ('rows', 'prologue_length')} for t in ct]
    verify = run(['dwarfdump', '--verify', keel])
    reference = run(['dwarfdump', '--verify', clang])
    # Clang's own objects can fail verification (e.g. .debug_names base-type
    # entries); then Keel must report exactly the same diagnostics.
    norm = lambda r, p: [x.replace(str(p), 'OBJ') for x in (r['stdout'] + r['stderr']).splitlines()]
    return {
        'sections': sorted(sa), 'section_mismatches': mismatches, 'known_preexisting_differences': known,
        # Section-start `ltmp` locals differ in name and placement before and
        # after this change (Mach-O writer); every other symbol must match.
        'symbols_equal': named(a) == named(b),
        'section_start_symbols': {'keel': len(a['symbols']) - len(named(a)),
                                  'clang': len(b['symbols']) - len(named(b))},
        'line_tables': len(kt), 'versions': sorted({t.get('version') for t in kt}),
        'directories': sum(len(t['directories']) for t in kt), 'files': sum(len(t['files']) for t in kt),
        'md5_files': sum(1 for t in kt for f in t['files'].values() if 'md5_checksum' in f),
        'source_files': sum(1 for t in kt for f in t['files'].values() if 'source' in f),
        'prologue_length_equal': [t.get('prologue_length') for t in kt] == [t.get('prologue_length') for t in ct],
        'header_files_directories_equal': headers_equal,
        'rows': sum(len(t['rows']) for t in kt),
        'rows_equal': [t['rows'] for t in kt] == [t['rows'] for t in ct],
        'resolved_rows_equal': [resolved(t) for t in kt] == [resolved(t) for t in ct],
        'debug_line_bytes': {'keel': sa.get(GENERATED, {}).get('size'), 'clang': sb.get(GENERATED, {}).get('size')},
        'keel_verify_clean': verify['returncode'] == 0 and 'No errors.' in verify['stdout'],
        'clang_verify_clean': reference['returncode'] == 0 and 'No errors.' in reference['stdout'],
        'verify_output_equal': norm(verify, keel) == norm(reference, clang),
        'keel_verify_ok': (verify['returncode'] == 0 and 'No errors.' in verify['stdout'])
        or (verify['returncode'] == reference['returncode'] and norm(verify, keel) == norm(reference, clang)),
        'verify_tail': verify['stdout'].strip().splitlines()[-1:] if verify['stdout'] else verify['stderr'][-400:],
    }


def case(job):
    name, source, flags, work, keel = job
    s = work / f'{name}.s'
    c, k = work / f'{name}.clang.o', work / f'{name}.keel.o'
    row = {'name': name, 'source': str(source), 'flags': flags}
    steps = [run(['clang', '-S', *TARGET, *flags, source, '-o', s]),
             run(['clang', '-c', *TARGET, s, '-o', c]),
             run([keel, 'asm', '--cpu', 'apple-m5', s, '-o', k])]
    row['steps'] = [{'cmd': r['cmd'], 'returncode': r['returncode'], 'stderr': r['stderr'][-600:]} for r in steps]
    if any(r['returncode'] for r in steps):
        row['ok'] = False
        return row
    row['assembly_bytes'] = s.stat().st_size
    row['file_directives'] = sum(1 for line in s.open(errors='replace') if line.lstrip().startswith('.file'))
    row.update(compare_objects(k, c))
    row['ok'] = (not row['section_mismatches'] and row['symbols_equal'] and row['header_files_directories_equal']
                 and row['rows_equal'] and row['resolved_rows_equal'] and row['keel_verify_ok']
                 and row['prologue_length_equal'])
    return row


def link(rows, work):
    """Link Lua from both object sets; compare dSYM line tables and run it."""
    lua = [r for r in rows if r['name'].startswith('lua-O2-')]
    if not all(r.get('ok') for r in lua):
        return {'ok': False, 'skipped': 'not every Lua -O2 case assembled and matched'}
    out = {}
    for engine in ('keel', 'clang'):
        exe = work / f'lua-{engine}'
        objs = [work / f'{r["name"]}.{engine}.o' for r in lua]
        steps = [run(['clang', *TARGET[:2], *objs, '-lm', '-o', exe]), run(['dsymutil', exe]),
                 run(['dwarfdump', '--verify', f'{exe}.dSYM']),
                 run([exe, '-e', 'local t={} for i=1,10 do t[i]=i*i end print(t[10], #t)'])]
        out[engine] = {'steps': [{'cmd': r['cmd'], 'returncode': r['returncode'], 'stdout': r['stdout'][-300:],
                                  'stderr': r['stderr'][-300:]} for r in steps]}
        out[engine]['tables'] = line_tables(Path(f'{exe}.dSYM'))
    kt, ct = out['keel'].pop('tables'), out['clang'].pop('tables')
    out['dsym_line_tables'] = len(kt)
    out['dsym_rows'] = sum(len(t['rows']) for t in kt)
    out['dsym_headers_equal'] = [{k: v for k, v in t.items() if k != 'rows'} for t in kt] == \
        [{k: v for k, v in t.items() if k != 'rows'} for t in ct]
    out['dsym_rows_equal'] = [t['rows'] for t in kt] == [t['rows'] for t in ct]
    exes = [work / 'lua-keel', work / 'lua-clang']
    out['executables_equal'] = all(e.exists() for e in exes) and exes[0].read_bytes() == exes[1].read_bytes()
    out['ok'] = (out['dsym_headers_equal'] and out['dsym_rows_equal']
                 and all(s['returncode'] == 0 for e in ('keel', 'clang') for s in out[e]['steps'])
                 and out['keel']['steps'][3]['stdout'] == out['clang']['steps'][3]['stdout'])
    return out


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--keel', type=Path, required=True)
    p.add_argument('--work', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--extra', type=Path, action='append', default=[], help='additional C files (-O1, embedded source)')
    p.add_argument('--large', action='store_true', help='include SQLite and QuickJS amalgamations')
    p.add_argument('--link', action='store_true')
    p.add_argument('--jobs', type=int, default=4)
    a = p.parse_args()
    a.work.mkdir(parents=True, exist_ok=True)
    lua = ROOT / 'target/suite/lua/lua-5.4.7/src'
    jobs = []
    for opt in ('-O0', '-O2'):
        for src in sorted(lua.glob('*.c')):
            if src.name in ('luac.c',):
                continue
            jobs.append((f'lua{opt}-{src.stem}', src, ['-gdwarf-5', opt, '-std=gnu99', '-DLUA_COMPAT_5_3',
                                                       '-DLUA_USE_MACOSX'], a.work, a.keel))
    if a.large:
        jobs.append(('sqlite-O2', ROOT / 'target/suite/sqlite/sqlite-amalgamation-3460100/sqlite3.c',
                     ['-gdwarf-5', '-O2'], a.work, a.keel))
        qjs = ROOT / 'target/suite/quickjs/quickjs-2024-01-13'
        jobs.append(('quickjs-O2', qjs / 'quickjs.c',
                     ['-gdwarf-5', '-O2', '-DCONFIG_VERSION="2024-01-13"', '-D_GNU_SOURCE', f'-I{qjs}'],
                     a.work, a.keel))
    for src in a.extra:
        jobs.append((f'extra-{src.stem}-O1', src, ['-gdwarf-5', '-O1'], a.work, a.keel))
        jobs.append((f'extra-{src.stem}-embed', src, ['-gdwarf-5', '-O0', '-gembed-source'], a.work, a.keel))
    with concurrent.futures.ThreadPoolExecutor(a.jobs) as pool:
        rows = list(pool.map(case, jobs))
    report = {'cases': rows, 'cases_ok': sum(r['ok'] for r in rows), 'cases_total': len(rows)}
    if a.link:
        report['link'] = link(rows, a.work)
    a.output.parent.mkdir(parents=True, exist_ok=True)
    a.output.write_text(json.dumps(report, indent=1, sort_keys=True))
    bad = [r['name'] for r in rows if not r['ok']]
    print(f"{report['cases_ok']}/{len(rows)} cases ok; failing: {bad}")
    if a.link:
        print('link ok' if report['link']['ok'] else f"link FAILED {json.dumps(report['link'])[:2000]}")
    return 0 if not bad and (not a.link or report['link']['ok']) else 1


if __name__ == '__main__':
    sys.exit(main())
