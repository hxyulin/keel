#!/usr/bin/env python3
"""Before/after evidence for the AArch64 assembler compatibility features.

Reads the frozen investigation evidence (inputs, retained Clang objects and
benchmark.json commands) without modifying it; writes only to --output.

Phases:
- rejected: the three previously rejected unchanged Clang AArch64 sources are
  assembled by the before and after binaries; after objects are compared with
  freshly re-run direct cc1as objects (themselves checked against the retained
  ones) by audit_clang_assembler.macho via assembler_investigate.inventory and
  compare, plus full symbol tables, masked section flags, unwind bytes, and
  resolved-path DWARF line rows with every row flag, and dwarfdump --verify.
- regression: every previously accepted primary input must produce an object
  byte-identical to the frozen before binary.
- forms: every distinct non-symbolic instruction form of the Clang QuickJS
  and SQLite sources, alone, against Clang.
- negative: CLI diagnostics for malformed inputs.
- speed: a bounded, paired, interleaved process-wall check of frozen, rebuilt
  HEAD control and after binaries on accepted inputs that reach changed code,
  plus after versus direct cc1as on the newly accepted inputs.
"""
import argparse
import concurrent.futures
import hashlib
import json
from pathlib import Path
import random
import re
import shutil
import statistics
import struct
import subprocess
import sys
import tempfile
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
from assembler_analyze import frame_rows  # noqa: E402
from assembler_investigate import compare, inventory, serializable  # noqa: E402
from audit_clang_assembler import macho  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
EVIDENCE = ROOT / 'target/assembler-investigation-20261019'
REJECTED = ['quickjs-quickjs-clang-O2', 'sqlite-sqlite3-clang-O2', 'arm64-functions-clang-debug']
SPEED = ['arm64-mixed-100000', 'lua-lvm-clang-O2', 'lua-lvm-keel-O', 'quickjs-quickjs-keel-O',
         'sqlite-sqlite3-keel-O', 'arm64-functions-clang', 'arm64-functions-keel-debug']
# Mach-O S_ATTR_EXT_RELOC|S_ATTR_LOC_RELOC: Keel sets EXT_RELOC on relocated
# sections where this Clang does not; recorded in the investigation.
RELOC_ATTRS = 0x300
UNWIND = ('__LD,__compact_unwind', '__TEXT,__eh_frame')


def sha(data):
    return hashlib.sha256(data).hexdigest()


def invoke(cmd, timeout=120):
    start = time.perf_counter_ns()
    p = subprocess.run(list(map(str, cmd)), capture_output=True, timeout=timeout)
    return {'cmd': list(map(str, cmd)), 'returncode': p.returncode,
            'stdout': p.stdout.decode(errors='replace'), 'stderr': p.stderr.decode(errors='replace'),
            'wall_ms': (time.perf_counter_ns() - start) / 1e6}


def keel_cmd(row, binary, out):
    cmd = list(row['commands']['keel'])
    cmd[0] = str(binary)
    cmd[-1] = str(out)
    return cmd


def clang_cmd(row, out):
    cmd = list(row['commands']['clang-cc1as'])
    cmd[-1] = str(out)
    return cmd


def line_table(path):
    """Decoded rows with each file index resolved through its directory."""
    text = invoke(['dwarfdump', '--debug-line', path])['stdout']
    dirs, files, rows, current = {0: ''}, {}, [], None
    for line in text.splitlines():
        if m := re.match(r'include_directories\[\s*(\d+)\] = "(.*)"', line):
            dirs[int(m[1])] = m[2]
        elif m := re.match(r'file_names\[\s*(\d+)\]:', line):
            current = int(m[1])
            files[current] = {}
        elif current is not None and (m := re.match(r'\s+(name|dir_index): (.*)', line)):
            files[current][m[1]] = m[2].strip('"')
        elif line.startswith('0x'):
            parts = line.split()
            path = files[int(parts[3])]
            directory = dirs[int(path['dir_index'])]
            resolved = f"{directory}/{path['name']}" if directory else path['name']
            rows.append([int(parts[0], 16), int(parts[1]), int(parts[2]), resolved,
                         int(parts[4]), int(parts[5]), int(parts[6]), sorted(parts[7:])])
    return {'directories': dirs, 'files': files, 'rows': rows}


def compact_entries(path):
    section = next(s for s in macho(Path(path))[0] if s['name'] == '__LD,__compact_unwind')
    return [struct.unpack_from('<QIIQQ', section['data'], i)[1:3] for i in range(0, len(section['data']), 32)]


def frameless_state(encoding):
    """libunwind's AArch64 frameless layout: pairs stored downward from CFA."""
    stack = ((encoding >> 12) & 0xfff) * 16
    regs, offset = {}, 0
    pairs = [(f'W{19 + 2 * i}', f'W{20 + 2 * i}') for i in range(5)] + \
            [(f'D{8 + 2 * i}', f'D{9 + 2 * i}') for i in range(4)]
    for bit, (first, second) in zip([0, 1, 2, 3, 4, 8, 9, 10, 11], pairs):
        if encoding & (1 << bit):
            regs[first], regs[second] = f'[CFA-{offset + 8}]', f'[CFA-{offset + 16}]'
            offset += 16
    return f'WSP+{stack}' if stack else 'WSP', regs


def parse_state(row):
    cfa, _, rest = row.partition(': ')
    return cfa.removeprefix('CFA='), dict(r.split('=', 1) for r in rest.split(', ') if r)


def unwind(keel, clang):
    """Compact entries equal, except where Clang's compact frameless encoding is
    represented by Keel as DWARF mode plus an FDE with the same final state;
    every other FDE must decode identically after rebasing."""
    kc, cc = compact_entries(keel), compact_entries(clang)
    eh = lambda p: frame_rows(invoke(['dwarfdump', '--eh-frame', p])['stdout'])
    kf, cf = eh(keel), eh(clang)
    dwarf = [i for i, (_, e) in enumerate(kc) if (e >> 24) & 15 == 3]
    result = {'compact_entries': len(cc), 'keel_fdes': len(kf), 'clang_fdes': len(cf),
              'same_entry_count': len(kc) == len(cc), 'fde_per_dwarf_mode_entry': len(dwarf) == len(kf)}
    if not (result['same_entry_count'] and result['fde_per_dwarf_mode_entry']):
        return result
    fde = dict(zip(dwarf, kf))
    differing = [i for i, (a, b) in enumerate(zip(kc, cc)) if a != b]
    checks = []
    for i in differing:
        (klen, kenc), (clen, cenc) = kc[i], cc[i]
        ok = klen == clen and (kenc >> 24) & 15 == 3 and (cenc >> 24) & 15 == 2 and i in fde
        final = fde[i]['rows'][-1][1] if ok and fde[i]['rows'] else None
        ok = ok and parse_state(final) == frameless_state(cenc)
        checks.append({'entry': i, 'keel_encoding': hex(kenc), 'clang_encoding': hex(cenc),
                       'keel_fde_final_row': final, 'equivalent': bool(ok)})
    result['differing_entries'] = checks
    result['differences_equivalent'] = all(c['equivalent'] for c in checks)
    result['remaining_fdes_equal'] = [f for i, f in fde.items() if i not in differing] == cf
    return result


def parity(keel, clang):
    a, b = inventory(keel), inventory(clang)
    result = compare(a, b)
    # Section-start locals: Clang's ltmpN and Keel's metadata section labels.
    synthetic = lambda s: re.fullmatch(r'ltmp\d+|ltmp_Lkeel_metadata_section\d+', s['name'])
    result['symbols_equal_except_synthetic_section_labels'] = (
        [s for s in a['symbols'] if not synthetic(s)] == [s for s in b['symbols'] if not synthetic(s)])
    result['synthetic_section_labels'] = {'keel': sum(map(bool, map(synthetic, a['symbols']))),
                                          'clang': sum(map(bool, map(synthetic, b['symbols'])))}
    sa = {s['name']: s for s in a['sections']}
    sb = {s['name']: s for s in b['sections']}
    result['sections_present_equal'] = sorted(sa) == sorted(sb)
    result['section_flags_equal_masking_reloc_attrs'] = all(
        sa[n]['flags'] & ~RELOC_ATTRS == sb[n]['flags'] & ~RELOC_ATTRS for n in sa.keys() & sb.keys())
    nondebug = [n for n in sa.keys() & sb.keys() if not n.startswith('__DWARF,') or n == '__DWARF,__debug_line']
    explicit = [n for n in sa.keys() & sb.keys() if n.startswith('__DWARF,') and n != '__DWARF,__debug_line']
    for label, names in [('code_data_unwind', [n for n in nondebug if n != '__DWARF,__debug_line']),
                         ('explicit_debug', explicit)]:
        result[f'{label}_bytes_equal'] = all(sa[n]['data'] == sb[n]['data'] for n in names)
        result[f'{label}_sizes_equal'] = all(sa[n]['size'] == sb[n]['size'] for n in names)
        result[f'{label}_relocations_equal'] = all(sa[n]['relocations'] == sb[n]['relocations'] for n in names)
        result[f'{label}_alignment_sufficient'] = all(sa[n]['alignment'] >= sb[n]['alignment'] for n in names)
        result[f'{label}_sections'] = sorted(names)
    result['code_data_bytes_equal'] = all(sa[n]['data'] == sb[n]['data'] for n in nondebug
                                          if n not in UNWIND and n != '__DWARF,__debug_line')
    result['code_data_relocations_equal'] = all(sa[n]['relocations'] == sb[n]['relocations'] for n in nondebug
                                                if n not in UNWIND and n != '__DWARF,__debug_line')
    if '__LD,__compact_unwind' in sa:
        result['unwind'] = unwind(keel, clang)
    if '__DWARF,__debug_line' in sa or '__DWARF,__debug_line' in sb:
        la, lb = line_table(keel), line_table(clang)
        result['line_rows'] = len(lb['rows'])
        result['line_rows_equal_with_resolved_paths'] = la['rows'] == lb['rows']
        result['line_directories_files_equal'] = (la['directories'], la['files']) == (lb['directories'], lb['files'])
        result['line_flag_counts'] = {f: sum(f in r[7] for r in lb['rows'])
                                      for f in ('is_stmt', 'prologue_end', 'epilogue_begin', 'end_sequence')}
        result['debug_line_bytes'] = {'keel': sa['__DWARF,__debug_line']['size'],
                                      'clang': sb['__DWARF,__debug_line']['size']}
        verify = invoke(['dwarfdump', '--verify', keel])
        result['keel_dwarfdump_verify'] = {'returncode': verify['returncode'],
                                           'tail': verify['stdout'].strip().splitlines()[-1:]}
    return result, serializable(a), serializable(b)


def forms(binary, sources, work):
    """Each distinct non-symbolic form alone, against Clang -mcpu=apple-m5."""
    ins = re.compile(r'^\s+([a-z][a-z0-9.]*)\b\s*(.*?)\s*(?:;.*)?$')
    found = {}
    for path in sources:
        for line in path.read_text().splitlines():
            m = ins.match(line)
            if not m or re.search(r'(?:\bL|_)[\w$.]+', m[2]):
                continue
            norm = re.sub(r'\[[0-9]+\]', '[i]', m[1] + ' ' + re.sub(
                r'#-?(0x)?[0-9a-f.]+', '#I', re.sub(r'\b([vxwqdsbhz])[0-9]+\b', r'\1N', m[2])))
            found.setdefault(norm, [0, f'{m[1]}\t{m[2]}'])[0] += 1

    def check(item):
        norm, (count, example) = item
        d = Path(tempfile.mkdtemp(dir=work))
        (d / 'f.s').write_text('\t.text\n' + example + '\n')
        k = invoke([binary, 'asm', '--cpu', 'apple-m5', d / 'f.s', '-o', d / 'k.o'])
        c = invoke(['clang', '-target', 'arm64-apple-macos11', '-mcpu=apple-m5', '-c', '-x', 'assembler',
                    d / 'f.s', '-o', d / 'c.o'])
        text = lambda o: invoke(['otool', '-t', o])['stdout'].split('\n', 2)[2].split()
        if k['returncode']:
            status = 'keel-reject: ' + k['stderr'].strip().split(': ')[-1]
        elif c['returncode']:
            status = 'clang-reject'
        else:
            status = 'equal' if text(d / 'k.o') == text(d / 'c.o') else 'differ'
        shutil.rmtree(d)
        return {'form': norm, 'occurrences': count, 'example': example, 'status': status}
    work.mkdir(parents=True, exist_ok=True)
    with concurrent.futures.ThreadPoolExecutor(8) as pool:
        return list(pool.map(check, sorted(found.items())))


NEGATIVE = {
    'dup-gpr-width': 'dup.2d v0, w8',
    'dup-lane-qualified': 'dup.2d v0, v1.d[0]',
    'dup-bad-register': 'dup.4s v32, w0',
    'dup-bad-suffix': 'dup.3s v0, w0',
    'tbl-qualified-list': 'tbl.16b v0, { v1.16b }, v2',
    'narrow-bad-suffix': 'xtn.16b v0, v1',
    'widen-bad-suffix': 'saddw.8b v0, v1, v2',
    'unknown-apple-root': 'sqadd.4s v0, v1, v2',
    'udot-without-dotprod': None,  # filled from source without --cpu below
    # `.file 0` now selects a DWARF 5 table; without a directory it stays an error.
    'file-dwarf5-zero': '.file 0 "a.c"',
    'file-md5': '.file 1 "/dir" "a.c" md5 0x00112233445566778899aabbccddeeff',
    'file-trailing': '.file 1 "/dir" "a.c" junk',
    'file-unterminated': '.file 1 "/dir',
    'file-unnumbered': '.file "a.c"',
    'file-duplicate': '.file 1 "a" "b"\n.file 1 "c" "d"',
    'loc-is-stmt-2': '.file 1 "a" "b"\n.loc 1 1 0 is_stmt 2',
    'loc-is-stmt-missing': '.file 1 "a" "b"\n.loc 1 1 0 is_stmt',
    'loc-unknown-subdirective': '.file 1 "a" "b"\n.loc 1 1 0 view 0',
    'loc-negative-line': '.file 1 "a" "b"\n.loc 1 -1 0',
    'loc-file-zero': '.file 1 "a" "b"\n.loc 0 1 0',
    'loc-undeclared': '.file 1 "a" "b"\n.loc 2 1 0\nret',
    'zerofill-other-section': '.zerofill __DATA,__data,_x,8,3',
}


def negative(binary, work):
    results = {}
    work.mkdir(parents=True, exist_ok=True)
    for name, body in NEGATIVE.items():
        args = ['--cpu', 'apple-m5']
        if body is None:
            body, args = 'udot.4s v0, v1, v2', []
        path = work / f'{name}.s'
        path.write_text('\t.text\n' + body + '\n')
        r = invoke([binary, 'asm', *args, path, '-o', work / f'{name}.o'])
        results[name] = {'source': body, 'returncode': r['returncode'], 'stderr': r['stderr'].strip()}
    return results


SMOKE = r"""
.file 1 "/tmp" "smoke.c"
.text
.globl _main
.p2align 2
_main:
.loc 1 1 0
    mov w8, #3
    dup.4s v0, w8
    addv.4s s1, v0
    fmov w9, s1
.loc 1 2 0 is_stmt 0
    fmov d2, #2.5
    frinta d3, d2
    fcvtzs w10, d3
    mov w11, #5
    mov w12, #7
    mov x13, #100
.loc 1 3 0 epilogue_begin
    smsubl x14, w11, w12, x13
    add w0, w9, w10
    add w0, w0, w14
    ret
.zerofill __DATA,__common,_unused,8,3
"""


def smoke(binary, work):
    """CLI assemble, Clang link and run: 12 + 3 + (100 - 35) = 80."""
    work.mkdir(parents=True, exist_ok=True)
    (work / 'smoke.s').write_text(SMOKE)
    steps = {'assemble': invoke([binary, 'asm', work / 'smoke.s', '-o', work / 'smoke.o'])}
    steps['verify'] = invoke(['dwarfdump', '--verify', work / 'smoke.o'])
    steps['link'] = invoke(['clang', '-target', 'arm64-apple-macos11', work / 'smoke.o', '-o', work / 'smoke'])
    steps['run'] = invoke([work / 'smoke'])
    return {'expected_exit': 80, 'steps': {k: {'returncode': v['returncode'], 'stderr': v['stderr'].strip()}
                                           for k, v in steps.items()},
            'passed': steps['run']['returncode'] == 80 and all(
                steps[k]['returncode'] == 0 for k in ('assemble', 'verify', 'link'))}


def timed(cmd):
    start = time.perf_counter_ns()
    p = subprocess.run(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
    elapsed = (time.perf_counter_ns() - start) / 1e6
    if p.returncode:
        raise RuntimeError(f'{cmd!r}: {p.stderr.decode(errors="replace")}')
    return elapsed


def paired(commands, warmups, rounds, seed):
    """Interleaved rounds; each round runs every engine once in shuffled order."""
    rng = random.Random(seed)
    for _ in range(warmups):
        for cmd in commands.values():
            timed(cmd)
    samples, orders = {k: [] for k in commands}, []
    for _ in range(rounds):
        order = list(commands)
        rng.shuffle(order)
        orders.append(order)
        for k in order:
            samples[k].append(timed(commands[k]))
    return samples, orders


def bootstrap_ratio(a, b, seed, n=4000):
    """Percentile interval of median(a)/median(b) over independent resamples."""
    rng = random.Random(seed)
    values = sorted(statistics.median(rng.choices(a, k=len(a))) /
                    statistics.median(rng.choices(b, k=len(b))) for _ in range(n))
    return [values[int(n * 0.025)], values[int(n * 0.975)]]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--before', type=Path, default=EVIDENCE / 'keel-frozen')
    parser.add_argument('--control', type=Path, required=True, help='HEAD rebuilt like --after')
    parser.add_argument('--after', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--rounds', type=int, default=12)
    parser.add_argument('--warmups', type=int, default=2)
    parser.add_argument('--skip-speed', action='store_true')
    args = parser.parse_args()
    out = args.output
    if out.resolve() == EVIDENCE.resolve():
        raise SystemExit('refusing to write into the frozen investigation evidence')
    out.mkdir(parents=True, exist_ok=True)
    bench = {r['name']: r for r in json.loads((EVIDENCE / 'benchmark.json').read_text())}
    binaries = {k: {'path': str(p), 'sha256': sha(p.read_bytes())}
                for k, p in (('before', args.before), ('control', args.control), ('after', args.after))}
    report = {'binaries': binaries, 'evidence': str(EVIDENCE), 'inputs': {}}
    for name, row in bench.items():
        data = Path(row['source']).read_bytes()
        assert sha(data) == row['source_sha256'], f'{name} input changed'
        report['inputs'][name] = {'source': row['source'], 'sha256': row['source_sha256']}

    rejected = {}
    for name in REJECTED:
        row, d = bench[name], out / 'objects' / name
        d.mkdir(parents=True, exist_ok=True)
        before = invoke(keel_cmd(row, args.before, d / 'before.o'))
        after = invoke(keel_cmd(row, args.after, d / 'after.o'))
        clang = invoke(clang_cmd(row, d / 'clang-cc1as.o'))
        retained = (EVIDENCE / 'objects' / name / 'clang-cc1as.o').read_bytes()
        entry = {'before': {k: before[k] for k in ('cmd', 'returncode', 'stderr')},
                 'after': {k: after[k] for k in ('cmd', 'returncode', 'stderr')},
                 'clang': {k: clang[k] for k in ('cmd', 'returncode', 'stderr')},
                 'clang_matches_retained_object': (d / 'clang-cc1as.o').read_bytes() == retained}
        if after['returncode'] == 0 and clang['returncode'] == 0:
            entry['parity'], ka, kb = parity(d / 'after.o', d / 'clang-cc1as.o')
            (d / 'inventory.json').write_text(json.dumps({'keel': ka, 'clang': kb}, indent=1) + '\n')
        rejected[name] = entry
        print(name, 'after rc', after['returncode'], flush=True)
    report['rejected'] = rejected

    regression = {}
    for name, row in bench.items():
        if name in REJECTED or row['initial_results']['keel']['returncode'] != 0:
            continue
        with tempfile.TemporaryDirectory(dir=out) as t:
            b = invoke(keel_cmd(row, args.before, Path(t) / 'b.o'))
            a = invoke(keel_cmd(row, args.after, Path(t) / 'a.o'))
            same = (b['returncode'] == a['returncode'] == 0 and
                    (Path(t) / 'b.o').read_bytes() == (Path(t) / 'a.o').read_bytes())
            regression[name] = {'identical': same, 'sha256': sha((Path(t) / 'a.o').read_bytes()) if a['returncode'] == 0 else None,
                                'retained_keel_object_equal': (EVIDENCE / 'objects' / name / 'keel.o').exists() and same and
                                (EVIDENCE / 'objects' / name / 'keel.o').read_bytes() == (Path(t) / 'a.o').read_bytes()}
    report['regression'] = regression
    print('regression identical', sum(r['identical'] for r in regression.values()), '/', len(regression), flush=True)

    report['forms'] = forms(args.after, [Path(bench[n]['source']) for n in REJECTED[:2]], out / 'forms-work')
    shutil.rmtree(out / 'forms-work', ignore_errors=True)
    statuses = [f['status'] for f in report['forms']]
    report['forms_summary'] = {s: statuses.count(s) for s in sorted(set(statuses))}
    print('forms', report['forms_summary'], flush=True)
    report['negative'] = negative(args.after, out / 'negative')
    report['negative_all_rejected'] = all(r['returncode'] == 1 and r['stderr'] for r in report['negative'].values())
    report['smoke'] = smoke(args.after, out / 'smoke')
    print('smoke passed', report['smoke']['passed'], flush=True)

    if not args.skip_speed:
        speed = {}
        for i, name in enumerate(SPEED + REJECTED):
            row = bench[name]
            t = out / 'speed-objects'
            t.mkdir(exist_ok=True)
            if name in REJECTED:
                commands = {'after': keel_cmd(row, args.after, t / f'{name}-after.o'),
                            'clang-cc1as': clang_cmd(row, t / f'{name}-clang.o')}
            else:
                commands = {k: keel_cmd(row, p, t / f'{name}-{k}.o')
                            for k, p in (('before', args.before), ('control', args.control), ('after', args.after))}
            samples, orders = paired(commands, args.warmups, args.rounds, seed=20261019 + i)
            med = {k: statistics.median(v) for k, v in samples.items()}
            entry = {'commands': commands, 'samples_ms': samples, 'orders': orders, 'median_ms': med}
            if name in REJECTED:
                entry['cc1as_over_after'] = med['clang-cc1as'] / med['after']
                entry['cc1as_over_after_95ci'] = bootstrap_ratio(samples['clang-cc1as'], samples['after'], i)
            else:
                for base in ('before', 'control'):
                    entry[f'after_over_{base}'] = med['after'] / med[base]
                    entry[f'after_over_{base}_95ci'] = bootstrap_ratio(samples['after'], samples[base], i)
            speed[name] = entry
            print('speed', name, {k: round(v, 3) for k, v in med.items()}, flush=True)
        report['speed'] = {'rounds': args.rounds, 'warmups': args.warmups, 'cases': speed}
    (out / 'features.json').write_text(json.dumps(report, indent=1) + '\n')


if __name__ == '__main__':
    main()
