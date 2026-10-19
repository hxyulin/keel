#!/usr/bin/env python3
"""Summarize retained assembler evidence; decode differences using Apple tools."""
import argparse
import json
from pathlib import Path
import random
import re
import statistics
import itertools
import time

from assembler_investigate import compare, digest, inventory, invoke, save, serializable
from measure_assembler import run


def line_rows(text):
    return [line.split() for line in text.splitlines() if re.match(r'^0x[0-9a-f]+\s+\d', line)]


def frame_rows(text):
    frames, frame, base = [], None, None
    for line in text.splitlines():
        match = re.search(r'FDE .*pc=([0-9a-f]+)\.\.\.([0-9a-f]+)', line)
        if match:
            base, end = (int(x, 16) for x in match.groups())
            frame = {'length': end - base, 'rows': []}
            frames.append(frame)
        match = re.match(r'\s+0x([0-9a-f]+): (CFA=.*)', line)
        if match and frame is not None:
            frame['rows'].append([int(match[1], 16) - base, match[2]])
    return frames


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--output', type=Path, required=True)
    args = ap.parse_args()
    work = args.output.resolve()
    rows = json.loads((work / 'benchmark.json').read_text())
    assert (work / 'clang-profile.json').exists(), 'Finish supplement first.'
    analysis = {'script_sha256': digest(Path(__file__).read_bytes()), 'timings': [], 'scaling': {}, 'decoded': []}
    rng = random.Random(20261019)
    for row in rows:
        if 'samples_ms' not in row:
            continue
        samples = row['samples_ms']
        ratios = [a / b for a, b in zip(samples['clang-cc1as'], samples['keel'])]
        boots = sorted(statistics.median(rng.choices(ratios, k=len(ratios))) for _ in range(4000))
        analysis['timings'].append({'name': row['name'], 'source_bytes': row['source_bytes'],
            'median_ms': {k: statistics.median(v) for k, v in samples.items()},
            'speedup_ratio_of_medians': row['speedup_clang_over_keel'],
            'cc1as_over_keel_median_paired_ratio': statistics.median(ratios),
            'cc1as_over_keel_paired_bootstrap_95': [boots[99], boots[3899]]})
    analysis['accepted'] = len(analysis['timings'])
    analysis['attempted'] = len(rows)
    analysis['rejected'] = [{'name': row['name'], 'errors': {k: v['stderr'] for k, v in row['initial_results'].items() if v['returncode']}}
                            for row in rows if 'samples_ms' not in row]
    for arch in ('arm64', 'x86'):
        selected = {r['instructions']: r for r in rows if r['name'].startswith(arch + '-mixed-')}
        empty = next(r for r in rows if r['name'] == arch + '-empty')
        low, high = selected[100000], selected[500000]
        deltas = {engine: high['distribution_ms'][engine]['median'] - low['distribution_ms'][engine]['median']
                  for engine in high['samples_ms']}
        analysis['scaling'][arch] = {
            'empty_process_median_ms': {k: v['median'] for k, v in empty['distribution_ms'].items()},
            'empty_driver_minus_cc1as_ms': empty['distribution_ms']['clang-driver']['median'] - empty['distribution_ms']['clang-cc1as']['median'],
            'slope_definition': 'Secant of medians: 100,000 to 500,000 repeated mixed instructions; includes incremental I/O and output work. Not a pure parser microbenchmark.',
            'incremental_ms': deltas,
            'incremental_ns_per_instruction': {k: v * 1e6 / 400000 for k, v in deltas.items()},
            'million_instructions_per_second': {k: 400 / v for k, v in deltas.items()},
            'decimal_source_MB_per_second': {k: (high['source_bytes'] - low['source_bytes']) / (v * 1000) for k, v in deltas.items()},
            'cc1as_over_keel_slope': deltas['clang-cc1as'] / deltas['keel']}

    # Decode ELF compiler output and retain full disassembly. Ignore only NOP
    # instructions, preserving addresses of every other instruction for equality.
    for name in ('x86-functions-keel', 'x86-functions-keel-debug'):
        decoded, disassembly = {}, {}
        for engine in ('keel', 'clang-cc1as'):
            obj = work / 'objects' / name / (engine + '.o')
            cmd = ['objdump', '-d', '--x86-asm-syntax=intel', '--no-show-raw-insn', str(obj)]
            result = invoke(cmd)
            save(obj.parent / (engine + '-disassembly.json'), {'command': cmd, **result})
            disassembly[engine] = []
            for line in result['stdout'].splitlines():
                m = re.match(r'\s*([0-9a-f]+):\s+(.*)', line)
                if m and not re.search(r'\bnop\b', m[2]):
                    disassembly[engine].append([int(m[1], 16), m[2]])
            decoded[engine] = {}
            for flag in ('--verify', '--debug-line', '--eh-frame'):
                cmd = ['dwarfdump', flag, str(obj)]
                result = invoke(cmd)
                save(obj.parent / (engine + flag + '.json'), {'command': cmd, **result})
                decoded[engine][flag] = result
        a = inventory(work / 'objects' / name / 'keel.o')
        b = inventory(work / 'objects' / name / 'clang-cc1as.o')
        text_a = next(s['data'] for s in a['sections'] if s['name'] == '.text')
        text_b = next(s['data'] for s in b['sections'] if s['name'] == '.text')
        analysis['decoded'].append({'name': name, 'non_nop_instructions_with_addresses_equal': disassembly['keel'] == disassembly['clang-cc1as'],
            'non_nop_instruction_count': len(disassembly['keel']),
            'text_size_equal': len(text_a) == len(text_b),
            'differing_text_bytes': sum(x != y for x, y in zip(text_a, text_b)),
            'frame_states_equal_after_rebasing': frame_rows(decoded['keel']['--eh-frame']['stdout']) == frame_rows(decoded['clang-cc1as']['--eh-frame']['stdout']),
            'fde_count': {k: len(frame_rows(v['--eh-frame']['stdout'])) for k, v in decoded.items()},
            'line_rows_equal': line_rows(decoded['keel']['--debug-line']['stdout']) == line_rows(decoded['clang-cc1as']['--debug-line']['stdout']),
            'line_row_count': {k: len(line_rows(v['--debug-line']['stdout'])) for k, v in decoded.items()},
            'verify_status': {k: v['--verify']['returncode'] for k, v in decoded.items()}})

    for name in ('probe-arm-lines', 'probe-arm-cfi-fallback', 'probe-arm-cfi-unsupported', 'arm64-functions-keel-debug'):
        decoded = {}
        for engine in ('keel', 'clang-cc1as'):
            p = work / 'objects' / name
            decoded[engine] = {flag: json.loads((p / (engine + flag + '.json')).read_text())['stdout']
                               for flag in ('--debug-line', '--eh-frame')}
        analysis['decoded'].append({'name': name,
            'line_rows_equal': line_rows(decoded['keel']['--debug-line']) == line_rows(decoded['clang-cc1as']['--debug-line']),
            'line_row_count': {k: len(line_rows(v['--debug-line'])) for k, v in decoded.items()},
            'frame_states_equal_after_rebasing': frame_rows(decoded['keel']['--eh-frame']) == frame_rows(decoded['clang-cc1as']['--eh-frame']),
            'fde_count': {k: len(frame_rows(v['--eh-frame'])) for k, v in decoded.items()}})
    stages = json.loads((work / 'keel-stages.json').read_text())
    analysis['keel_stages_median_ms'] = {r['name']: {k: statistics.median(r['result'][k]) for k in ('assemble_ms', 'macho_ms')}
                                         for r in stages['rows']}
    analysis['cpu_option_sensitivity'] = []
    base = json.loads((work / 'existing-baseline.json').read_text())
    m5 = json.loads((work / 'existing-apple-m5.json').read_text())
    for a, b in zip(base['rows'], m5['rows']):
        assert a['name'] == b['name']
        analysis['cpu_option_sensitivity'].append({'name': a['name'],
            'baseline_median_ms': a['median_ms'], 'apple_m5_median_ms': b['median_ms'],
            'm5_over_baseline': {k: b['median_ms'][k] / a['median_ms'][k] for k in a['median_ms']},
            'note': 'Existing script, randomized 12 repeats and 2 warmups; baseline selects Keel baseline and Clang apple-m1. Both runs use the same existing-script command shape; see exact commands in reports.'})

    # Independently reproduce the architecture audit's specific candidates.
    probe_specs = [
        ('arm-extern-defined', 'arm64', '.text\n.extern _foo\n_foo: ret\n'),
        ('arm-private-extern', 'arm64', '.text\n.private_extern _foo\n.globl _foo\n_foo: ret\n'),
        ('arm-loc-is-stmt', 'arm64', '.file 1 "p.c"\n.text\n.loc 1 1 0 is_stmt 0\nret\n'),
        ('arm-cfi-escape', 'arm64', '.text\n.cfi_startproc\n.cfi_escape 0\nret\n.cfi_endproc\n'),
        ('x86-byte300', 'x86', '.data\n.byte 300\n'),
        ('x86-octal', 'x86', '.data\n.long 010\n'),
        ('x86-section-default', 'x86', '.intel_syntax noprefix\n.section .text.foo\nnop\n'),
        ('x86-fill300', 'x86', '.text\n.byte 0\n.p2align 2,300\n'),
    ]
    semantic = []
    for name, arch, text in probe_specs:
        source = work / 'inputs' / ('probe-' + name + '.s')
        source.write_text(text)
        template = next(r for r in rows if r['name'] == arch + '-small')
        out = work / 'objects' / ('probe-' + name)
        out.mkdir(exist_ok=True)
        record = {'name': name, 'source_sha256': digest(source.read_bytes()), 'commands': {}, 'results': {}}
        invs = {}
        for engine, original in template['commands'].items():
            cmd = list(original)
            cmd[cmd.index(template['source'])] = str(source)
            cmd[-1] = str(out / (engine + '.o'))
            result = invoke(cmd)
            record['commands'][engine], record['results'][engine] = cmd, result
            if not result['returncode']:
                invs[engine] = inventory(Path(cmd[-1]))
                save(out / (engine + '.inventory.json'), serializable(invs[engine]))
        if 'keel' in invs and 'clang-cc1as' in invs:
            record['parity'] = compare(invs['keel'], invs['clang-cc1as'])
        semantic.append(record)
    save(work / 'semantic-probes.json', semantic)

    # The primary ELF driver defaults to DWARF 5. Explicitly match Keel's v4
    # for this debug comparison, changing only cc1as's version option.
    template = next(r for r in rows if r['name'] == 'x86-functions-keel-debug')
    cmds = {k: list(template['commands'][k]) for k in ('keel', 'clang-cc1as')}
    cc = cmds['clang-cc1as']
    at = next(i for i, value in enumerate(cc) if value.startswith('-dwarf-version='))
    cc[at] = '-dwarf-version=4'
    for engine, cmd in cmds.items():
        cmd[-1] = str(work / ('x86-dwarf4-' + engine + '.o'))
    for _ in range(2):
        for cmd in cmds.values():
            run(cmd)
    samples = {k: [] for k in cmds}
    orders = list(itertools.permutations(cmds)) * 6
    rng.shuffle(orders)
    for order in orders:
        for engine in order:
            start = time.perf_counter_ns()
            run(cmds[engine])
            samples[engine].append((time.perf_counter_ns() - start) / 1e6)
    invs, decoded = {}, {}
    for engine, cmd in cmds.items():
        invs[engine] = inventory(Path(cmd[-1]))
        save(work / ('x86-dwarf4-' + engine + '.inventory.json'), serializable(invs[engine]))
        decoded[engine] = {}
        for flag in ('--verify', '--debug-line', '--eh-frame'):
            result = invoke(['dwarfdump', flag, cmd[-1]])
            save(work / ('x86-dwarf4-' + engine + flag + '.json'), result)
            decoded[engine][flag] = result
    analysis['matched_x86_dwarf4'] = {
        'commands': cmds, 'samples_ms': samples, 'orders': orders, 'warmups': 2,
        'median_ms': {k: statistics.median(v) for k, v in samples.items()},
        'parity': compare(invs['keel'], invs['clang-cc1as']),
        'line_rows_equal': line_rows(decoded['keel']['--debug-line']['stdout']) == line_rows(decoded['clang-cc1as']['--debug-line']['stdout']),
        'line_row_count': {k: len(line_rows(v['--debug-line']['stdout'])) for k, v in decoded.items()},
        'verify_status': {k: v['--verify']['returncode'] for k, v in decoded.items()}}
    save(work / 'analysis.json', analysis)
    print(json.dumps({k: analysis[k] for k in ('accepted', 'attempted', 'scaling', 'decoded')}, indent=2))


if __name__ == '__main__':
    main()
