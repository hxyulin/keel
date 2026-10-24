#!/usr/bin/env python3
"""Bounded same-.s assembler investigation; never builds or edits the compiler.

Reuses measure_assembler.run and audit_clang_assembler.macho. Every command,
sample, input, object, diagnostic and normalized inventory is retained. Direct
cc1as commands are parsed from this machine's driver -### output, not guessed.
"""
import argparse
import hashlib
import itertools
import json
import os
from pathlib import Path
import platform
import random
import shlex
import shutil
import statistics
import struct
import subprocess
import sys
import time

from audit_clang_assembler import macho
from measure_assembler import run

ROOT = Path(__file__).resolve().parent.parent


def digest(data):
    return hashlib.sha256(data).hexdigest()


def save(path, value):
    path.write_text(json.dumps(value, indent=2) + '\n')


def invoke(cmd):
    start = time.perf_counter_ns()
    p = subprocess.run(list(map(str, cmd)), capture_output=True, timeout=60)
    return {'returncode': p.returncode, 'stdout': p.stdout.decode(errors='replace'),
            'stderr': p.stderr.decode(errors='replace'),
            'wall_ms': (time.perf_counter_ns() - start) / 1e6}


def cstr(data, at):
    return data[at:data.index(b'\0', at)].decode(errors='replace')


def inventory(path):
    """Existing normalized Mach-O audit plus raw symbol/command metadata; ELF64.

    Raw bytes remain in the retained .o; JSON uses sizes, hashes and prefixes.
    ELF relocation targets retain global/weak names; local targets normalize
    to section+offset. This is an audit, not a general object file library.
    """
    data = path.read_bytes()
    if data[:4] == b'\xcf\xfa\xed\xfe':
        secs, build, hints = macho(path)
        result = {'format': 'MachO', 'build_version': build, 'linker_hints': hints,
                  'header_flags': struct.unpack_from('<I', data, 24)[0]}
        offset, symbols, commands = 32, [], []
        for _ in range(struct.unpack_from('<I', data, 16)[0]):
            cmd, size = struct.unpack_from('<II', data, offset)
            commands.append({'cmd': cmd, 'size': size})
            if cmd == 2:
                symoff, count, strings, _ = struct.unpack_from('<IIII', data, offset + 8)
                for i in range(count):
                    name, kind, ordinal, desc, value = struct.unpack_from('<IBBHQ', data, symoff + i * 16)
                    section = secs[ordinal - 1] if ordinal else None
                    symbols.append({'name': cstr(data, strings + name), 'kind': kind,
                                    'binding': 'global' if kind & 1 else 'local', 'desc': desc,
                                    'section': section['name'] if section else None,
                                    'value': value - section['address'] if section else value})
            offset += size
        # Recover full section flags, not only the low type bits in old audit.
        offset = 32
        n = 0
        for _ in range(struct.unpack_from('<I', data, 16)[0]):
            cmd, size = struct.unpack_from('<II', data, offset)
            if cmd == 0x19:
                for i in range(struct.unpack_from('<I', data, offset + 64)[0]):
                    secs[n]['flags'] = struct.unpack_from('<I', data, offset + 72 + i * 80 + 64)[0]
                    secs[n]['alignment'] = 1 << secs[n]['alignment']
                    n += 1
            offset += size
        result['load_commands'] = commands
    else:
        assert data[:6] == b'\x7fELF\x02\x01', path
        result = {'format': 'ELF64', 'machine': struct.unpack_from('<H', data, 18)[0],
                  'header_flags': struct.unpack_from('<I', data, 48)[0]}
        shoff = struct.unpack_from('<Q', data, 40)[0]
        shsize, count, shstr = struct.unpack_from('<HHH', data, 58)
        headers = [struct.unpack_from('<IIQQQQIIQQ', data, shoff + i * shsize) for i in range(count)]
        strings = data[headers[shstr][4]:headers[shstr][4] + headers[shstr][5]]
        names = [cstr(strings, h[0]) for h in headers]
        symbols, symtabs = [], {}
        for index, h in enumerate(headers):
            if h[1] != 2:
                continue
            st = headers[h[6]]
            tab = data[st[4]:st[4] + st[5]]
            entries = []
            for at in range(h[4], h[4] + h[5], h[9]):
                name, info, other, section, value, size = struct.unpack_from('<IBBHQQ', data, at)
                entries.append({'name': cstr(tab, name), 'kind': info & 15,
                                'binding': info >> 4, 'visibility': other,
                                'section': names[section] if 0 < section < count else section,
                                'value': value, 'size': size})
            symtabs[index] = entries
            symbols.extend(entries[1:])
        secs = []
        for index, h in enumerate(headers):
            if not index:
                continue
            secs.append({'name': names[index], 'type': h[1], 'flags': h[2],
                         'size': h[5], 'alignment': h[8], 'entry_size': h[9],
                         'data': b'' if h[1] == 8 else data[h[4]:h[4] + h[5]],
                         'relocations': []})
        for h in headers:
            if h[1] != 4:
                continue
            for at in range(h[4], h[4] + h[5], h[9]):
                address, info, addend = struct.unpack_from('<QQq', data, at)
                sym = symtabs[h[6]][info >> 32]
                if sym['binding'] != 0 or not isinstance(sym['section'], str):
                    target = ['symbol', sym['name']]
                else:
                    target = ['section', sym['section']]
                    addend += sym['value']
                secs[h[7] - 1]['relocations'].append([address, info & 0xffffffff, addend, target])
        for s in secs:
            s['relocations'].sort()
    result['symbols'] = sorted(symbols, key=lambda s: json.dumps(s, sort_keys=True))
    result['sections'] = secs
    result['object_bytes'] = len(data)
    result['object_sha256'] = digest(data)
    return result


def serializable(inv):
    out = dict(inv)
    out['sections'] = [{**{k: v for k, v in s.items() if k not in ('data', 'reloc', 'address')},
                        'data_sha256': digest(s['data']), 'data_prefix_hex': s['data'][:64].hex()}
                       for s in inv['sections']]
    return out


def compare(a, b):
    rows = []
    sa = {s['name']: s for s in a['sections']}
    sb = {s['name']: s for s in b['sections']}
    for name in sorted(sa.keys() | sb.keys()):
        x, y = sa.get(name), sb.get(name)
        metadata = name.startswith(('__DWARF,', '.debug')) or name in (
            '__TEXT,__eh_frame', '__LD,__compact_unwind', '.eh_frame')
        structural = name in ('.symtab', '.strtab', '.shstrtab') or name.startswith('.rela')
        row = {'name': name, 'metadata': metadata, 'object_structure': structural,
               'present_keel': x is not None, 'present_clang': y is not None}
        if x is not None and y is not None:
            row.update({'bytes_equal': x['data'] == y['data'], 'size_equal': x['size'] == y['size'],
                        'type_equal': x['type'] == y['type'], 'flags_equal': x['flags'] == y['flags'],
                        'alignment_equal': x['alignment'] == y['alignment'],
                        'alignment_sufficient': x['alignment'] >= y['alignment'],
                        'relocations_equal': x['relocations'] == y['relocations']})
        rows.append(row)
    def exported(inv):
        return [s for s in inv['symbols'] if s['binding'] not in ('local', 0)]
    return {'sections': rows, 'all_symbols_equal': a['symbols'] == b['symbols'],
            'exported_symbols_equal': exported(a) == exported(b),
            'keel_only_symbols': [s for s in a['symbols'] if s not in b['symbols']],
            'clang_only_symbols': [s for s in b['symbols'] if s not in a['symbols']],
            'build_version_equal': a.get('build_version') == b.get('build_version'),
            'linker_hints_equal': a.get('linker_hints') == b.get('linker_hints')}


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--compiler', type=Path, required=True)
    ap.add_argument('--output', type=Path, required=True)
    ap.add_argument('--baseline-inputs', type=Path, default=ROOT / 'target/fleet-baseline/macos-arm64/measurement')
    ap.add_argument('--repeats', type=int, default=12)
    ap.add_argument('--warmups', type=int, default=2)
    ap.add_argument('--prepare-only', action='store_true')
    args = ap.parse_args()
    assert args.repeats >= 12 and args.repeats % 6 == 0 and args.warmups >= 2
    work = args.output.resolve()
    work.mkdir(parents=True, exist_ok=True)
    (work / 'inputs').mkdir(exist_ok=True)
    (work / 'objects').mkdir(exist_ok=True)
    compiler = work / 'keel-frozen'
    if not compiler.exists():
        shutil.copy2(args.compiler, compiler)
    assert digest(compiler.read_bytes()) == digest(args.compiler.read_bytes())
    clang = str(Path(shutil.which('clang')).resolve())
    save(work / 'environment.json', {
        'utc': time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime()), 'platform': platform.platform(),
        'uname': run(['uname', '-a']).decode(), 'cpu': run(['sysctl', '-n', 'machdep.cpu.brand_string']).decode(),
        'clang': run([clang, '--version']).decode(), 'clang_path': clang,
        'rustc': run(['rustc', '-Vv']).decode(), 'python': sys.version,
        'revision_now_not_binary_provenance': run(['git', 'rev-parse', 'HEAD']).decode().strip(),
        'git_status': run(['git', 'status', '--short']).decode(),
        'compiler_original': str(args.compiler.resolve()), 'compiler_sha256': digest(compiler.read_bytes()),
        'scripts_sha256': {p.name: digest(p.read_bytes()) for p in
                           [Path(__file__), ROOT / 'suite/measure_assembler.py', ROOT / 'suite/audit_clang_assembler.py']},
        'scope': 'Serial warm-cache subprocess wall incl startup, input read, output write, process exit; no preprocessing, C compilation, linking, or runtime.',
        'warmups_per_command': args.warmups, 'repeats_per_command': args.repeats,
        'order': 'Each of the six permutations twice (12 repeats), shuffled deterministically per input. Every engine appears equally often in each position.',
        'seed': 20261019, 'argv': sys.argv,
        'guidance': 'No AGENTS.md found in workspace (excluding target*) or direct ancestors.'})
    manifest = []

    def add(name, target, text=None, original=None, group='synthetic', instructions=None, flags=None):
        path = work / 'inputs' / (name + '.s')
        if original:
            shutil.copyfile(original, path)
        elif text is not None:
            path.write_text(text)
        manifest.append({'name': name, 'target': target, 'source': str(path), 'group': group,
                         'original': str(original) if original else None,
                         'instructions': instructions, 'flags': flags or [],
                         'source_bytes': path.stat().st_size, 'source_sha256': digest(path.read_bytes())})

    for arch in ('arm64', 'x86'):
        prefix = '.text\n' if arch == 'arm64' else '.intel_syntax noprefix\n.text\n'
        add(arch + '-empty', arch, prefix, instructions=0)
        add(arch + '-small', arch, prefix + ('mov x0,#42\nret\n' if arch == 'arm64' else 'mov eax,42\nret\n'), instructions=2)
        mixed = ('add x0,x1,#17\nsub w3,w4,w5,lsl #2\nldr x6,[x7,#24]\nstr d0,[sp,#16]\n'
                 'fadd d0,d1,d2\nmovz x8,#1234\nand x9,x10,x11\nnop\n') if arch == 'arm64' else (
                 'add rax,17\nsub r8,r9\nmov r10,[r11+24]\nmov [rsp+16],r12\n'
                 'addsd xmm0,xmm1\nmov r13,1234\nand r14,r15\nnop\n')
        for count in (1000, 10000, 100000, 500000):
            add(f'{arch}-mixed-{count}', arch, prefix + mixed * (count // 8), instructions=count)
    for app, stem in [('lua', 'lvm'), ('quickjs', 'quickjs'), ('sqlite', 'sqlite3')]:
        for producer in ('keel-O', 'clang-O2'):
            add(f'{app}-{stem}-{producer}', 'arm64', original=args.baseline_inputs / app / producer / (stem + '.s'), group='real')
    c = work / 'inputs/functions.i'
    c.write_text('extern long external(long);\n' + ''.join(
        f'long fn{i}(long x, long *p) {{ long y=p[x&7]+{i}; if(x>y)y=external(y); return y*3+x; }}\n'
        for i in range(512)))
    generation = []
    for arch in ('arm64', 'x86'):
        for producer in ('keel', 'clang'):
            for debug in (False, True):
                name = f'{arch}-functions-{producer}' + ('-debug' if debug else '')
                path = work / 'inputs' / (name + '.s')
                cmd = ([str(compiler), 'cc', '-O', *(['--target', 'x86_64-unknown-linux-gnu'] if arch == 'x86' else [])]
                       if producer == 'keel' else [clang, '-S', '-O2', '-target',
                           'arm64-apple-macos11' if arch == 'arm64' else 'x86_64-unknown-linux-gnu',
                           *(['-masm=intel'] if arch == 'x86' else [])])
                cmd += [*(['-g'] if debug else []), str(c), '-o', str(path)]
                outcome = invoke(cmd)
                generation.append({'command': cmd, **outcome})
                if not outcome['returncode']:
                    add(name, arch, group='generated-debug' if debug else 'generated')
    save(work / 'generation.json', generation)
    save(work / 'inputs.json', manifest)

    def commands(item):
        out = work / 'objects' / item['name']
        out.mkdir(exist_ok=True)
        arm = item['target'] == 'arm64'
        driver = [clang, '-target', 'arm64-apple-macos11' if arm else 'x86_64-unknown-linux-gnu',
                  *(['-mcpu=apple-m5'] if arm else []), '-x', 'assembler', '-c',
                  *item['flags'], item['source'], '-o', str(out / 'clang-driver.o')]
        trace = invoke([*driver, '-###'])
        (out / 'driver-###.txt').write_text(trace['stderr'])
        cc = next(shlex.split(line) for line in trace['stderr'].splitlines() if '"-cc1as"' in line)
        at = cc.index('-o')
        del cc[at:at + 2]
        cc += ['-o', str(out / 'clang-cc1as.o')]
        return {'keel': [str(compiler), 'asm', *(['--cpu', 'apple-m5'] if arm else ['--target', 'x86_64-unknown-linux-gnu']),
                         item['source'], '-o', str(out / 'keel.o')],
                'clang-driver': driver, 'clang-cc1as': cc}

    if args.prepare_only:
        return
    rng = random.Random(20261019)
    rows = []
    for item in manifest:
        cmds = commands(item)
        outcomes = {name: invoke(cmd) for name, cmd in cmds.items()}
        row = {**item, 'commands': cmds, 'initial_results': outcomes}
        rows.append(row)
        if all(v['returncode'] == 0 for v in outcomes.values()):
            invs = {name: inventory(Path(cmd[-1])) for name, cmd in cmds.items()}
            out = work / 'objects' / item['name']
            for name, inv in invs.items():
                save(out / (name + '.inventory.json'), serializable(inv))
            row['parity'] = compare(invs['keel'], invs['clang-cc1as'])
            row['driver_cc1as_object_bytes_equal'] = Path(cmds['clang-driver'][-1]).read_bytes() == Path(cmds['clang-cc1as'][-1]).read_bytes()
            for _ in range(args.warmups):
                for cmd in cmds.values():
                    run(cmd)
            samples = {name: [] for name in cmds}
            orders = list(itertools.permutations(cmds)) * (args.repeats // 6)
            rng.shuffle(orders)
            for order in orders:
                for name in order:
                    start = time.perf_counter_ns()
                    run(cmds[name])
                    samples[name].append((time.perf_counter_ns() - start) / 1e6)
            row['samples_ms'] = samples
            row['orders'] = orders
            row['distribution_ms'] = {name: {'min': min(v), 'median': statistics.median(v),
                'mean': statistics.mean(v), 'max': max(v), 'stdev': statistics.stdev(v),
                'q1': statistics.quantiles(v)[0], 'q3': statistics.quantiles(v)[2]} for name, v in samples.items()}
            row['speedup_clang_over_keel'] = {name: statistics.median(samples[name]) / statistics.median(samples['keel'])
                                             for name in cmds if name != 'keel'}
        save(work / 'benchmark.json', rows)
        print(item['name'], row.get('speedup_clang_over_keel', {k: v['stderr'][:200] for k, v in outcomes.items() if v['returncode']}), flush=True)

    arm_prefix = '.text\n.globl _probe\n_probe:\n'
    xprefix = '.intel_syntax noprefix\n.text\n.globl probe\n.type probe,@function\nprobe:\n'
    probes = [
        ('arm-cfi', 'arm64', arm_prefix + '.cfi_startproc\nstp x29,x30,[sp,#-16]!\nmov x29,sp\n.cfi_def_cfa x29,16\n.cfi_offset x29,-16\n.cfi_offset x30,-8\nbl _external\nldp x29,x30,[sp],#16\nret\n.cfi_endproc\n', []),
        ('arm-cfi-fallback', 'arm64', arm_prefix + '.cfi_startproc\n.cfi_def_cfa x29,16\n.cfi_offset x19,-24\nnop\n.cfi_remember_state\n.cfi_restore_state\nret\n.cfi_endproc\n', []),
        ('arm-lines', 'arm64', '.file 1 "example.c"\n' + arm_prefix + '.loc 1 4 2\nnop\n.loc 1 9 7 prologue_end\nret\n', []),
        ('arm-driver-g', 'arm64', arm_prefix + 'nop\nret\n', ['-g']),
        ('arm-align', 'arm64', arm_prefix + 'nop\n.p2align 5\nret\n.data\n.p2align 4\n.quad _external+9\n.comm _common,32,4\n', []),
        ('arm-unknown', 'arm64', '.keel_unknown_directive 1\n', []),
        ('arm-expression', 'arm64', '.data\n.long _external*_another\n', []),
        ('arm-b-overflow', 'arm64', '.text\nb #134217728\n', []),
        ('arm-b-misaligned', 'arm64', '.text\nb #2\n', []),
        ('arm-cond-overflow', 'arm64', '.text\nb.eq Lfar\n.space 1048576\nLfar: ret\n', []),
        ('arm-cfi-unsupported', 'arm64', arm_prefix + '.cfi_startproc\n.cfi_undefined x19\nret\n.cfi_endproc\n', []),
        ('x86-size-type-align', 'x86', xprefix + 'call external@PLT\nmov r8,[rip+external@GOTPCREL]\nlea rax,[rip+data+7]\nret\n.size probe,.-probe\n.data\n.p2align 4\n.type data,@object\ndata: .quad external+9\n.size data,8\n.section .bss,"aw",@nobits\n.p2align 5\n.zero 32\n.section .note.GNU-stack,"",@progbits\n', []),
        ('x86-relax-short', 'x86', xprefix + 'jmp Ldone\nje probe\n.zero 20\nLdone: ret\n.size probe,.-probe\n', []),
        ('x86-relax-long', 'x86', xprefix + 'jmp Ldone\nje probe\n.zero 256\nLdone: ret\n.size probe,.-probe\n', []),
        ('x86-unknown', 'x86', '.keel_unknown_directive 1\n', []),
        ('x86-expression', 'x86', '.data\n.quad external*another\n', []),
        ('x86-overflow', 'x86', xprefix + 'mov eax,4294967296\n', []),
    ]
    probe_rows = []
    for name, target, text, flags in probes:
        add('probe-' + name, target, text, group='probe', flags=flags)
        item = manifest[-1]
        cmds = commands(item)
        outcomes = {name: invoke(cmd) for name, cmd in cmds.items()}
        row = {**item, 'commands': cmds, 'results': outcomes}
        invs = {}
        out = work / 'objects' / item['name']
        for engine, result in outcomes.items():
            if result['returncode']:
                continue
            invs[engine] = inventory(Path(cmds[engine][-1]))
            save(out / (engine + '.inventory.json'), serializable(invs[engine]))
            if target == 'arm64':
                for flag in ('--verify', '--debug-line', '--eh-frame'):
                    dump = invoke(['dwarfdump', flag, cmds[engine][-1]])
                    save(out / (engine + flag + '.json'), dump)
        if 'keel' in invs and 'clang-cc1as' in invs:
            row['parity'] = compare(invs['keel'], invs['clang-cc1as'])
        probe_rows.append(row)
        print(item['name'], {k: v['returncode'] for k, v in outcomes.items()}, flush=True)
    save(work / 'probes.json', probe_rows)
    save(work / 'inputs.json', manifest)


if __name__ == '__main__':
    main()
