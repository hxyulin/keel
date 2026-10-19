#!/usr/bin/env python3
"""Follow up a completed assembler_investigate.py run, serially, outside timing.

Uses the existing measure_assembler.py and matching profile example. Also
retains decoded DWARF and a bounded Clang sample, and refreshes just the one
primary row that overlapped the private profiler build in the original run.
"""
import argparse
import hashlib
import itertools
import json
from pathlib import Path
import random
import shutil
import statistics
import subprocess
import time

from assembler_investigate import ROOT, compare, digest, inventory, invoke, save
from measure_assembler import run


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--output', type=Path, required=True)
    args = ap.parse_args()
    work = args.output.resolve()
    rows = json.loads((work / 'benchmark.json').read_text())
    assert len(rows) == 26, 'Run after the bounded primary benchmark completes.'
    compiler = work / 'keel-frozen'
    profiler = work / 'private-build/release/examples/profile'
    assert profiler.is_file()
    save(work / 'supplement-identity.json', {'script_sha256': digest(Path(__file__).read_bytes()),
         'profiler_sha256': digest(profiler.read_bytes()),
         'private_build_command': ['cargo', 'build', '--release', '--offline', '--locked',
             '--manifest-path', '/tmp/keel-machost-baseline-20261019.O6wiDXVg/src/Cargo.toml',
             '--target-dir', str(work / 'private-build'), '-p', 'keel-asm', '--example', 'profile'],
         'utc': time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime())})

    # Keep the original raw evidence; replace only the possibly affected row.
    pilot = work / 'benchmark-before-retime.json'
    if not pilot.exists():
        shutil.copyfile(work / 'benchmark.json', pilot)
    row = next(r for r in rows if r['name'] == 'arm64-mixed-500000')
    commands = row['commands']
    for _ in range(2):
        for cmd in commands.values():
            run(cmd)
    samples = {name: [] for name in commands}
    orders = list(itertools.permutations(commands)) * 2
    random.Random(20261020).shuffle(orders)
    for order in orders:
        for name in order:
            start = time.perf_counter_ns()
            run(commands[name])
            samples[name].append((time.perf_counter_ns() - start) / 1e6)
    row['samples_ms'], row['orders'] = samples, orders
    row['distribution_ms'] = {name: {'min': min(v), 'median': statistics.median(v),
        'mean': statistics.mean(v), 'max': max(v), 'stdev': statistics.stdev(v),
        'q1': statistics.quantiles(v)[0], 'q3': statistics.quantiles(v)[2]} for name, v in samples.items()}
    row['speedup_clang_over_keel'] = {name: statistics.median(samples[name]) / statistics.median(samples['keel'])
                                    for name in commands if name != 'keel'}
    row['retime_reason'] = 'Original row overlapped a 5.44-second private profiler build. Replaced by 2 warmups + 12 balanced repeats after all builds and primary timing finished. Original samples retained in benchmark-before-retime.json.'
    save(work / 'benchmark.json', rows)
    print('retimed', row['name'], flush=True)

    selected = ['arm64-empty', 'arm64-mixed-100000', 'lua-lvm-clang-O2']
    input_dir = work / 'existing-script-inputs'
    input_dir.mkdir(exist_ok=True)
    for name in selected:
        shutil.copyfile(work / 'inputs' / (name + '.s'), input_dir / (name + '.s'))
    supplementary = []
    for cpu in ['baseline', 'apple-m5']:
        cmd = ['python3', str(ROOT / 'suite/measure_assembler.py'), '--compiler', str(compiler),
               '--profiler', str(profiler), '--cpu', cpu, '--pairs', '12', '--warmups', '2',
               '--only-inputs', '--input-dir', str(input_dir),
               '--work-dir', str(work / ('existing-' + cpu)),
               '--output', str(work / ('existing-' + cpu + '.json'))]
        # These are bounded 3-input jobs; timeout allows their complete sample set.
        p = subprocess.run(cmd, capture_output=True, text=True, timeout=120)
        supplementary.append({'command': cmd, 'returncode': p.returncode, 'stdout': p.stdout, 'stderr': p.stderr})
        print('existing script', cpu, p.returncode, flush=True)
    save(work / 'supplement-commands.json', supplementary)

    stages = []
    for row in rows:
        if row['target'] != 'arm64' or not row.get('samples_ms'):
            continue
        cmd = [str(profiler), row['source'], '14', 'apple-m5']
        raw = json.loads(run(cmd))
        result = {k: v[2:] if isinstance(v, list) else v for k, v in raw.items()}
        stages.append({'name': row['name'], 'command': cmd,
                       'result': result, 'raw_result_including_two_discarded_warmups': raw,
                       'note': 'Existing profile.rs: initial warmup plus first two stage iterations discarded; 12 retained samples; alternating parse/encode and object-writer stage order; input I/O and result destruction excluded. Not a Clang stage comparison.'})
    save(work / 'keel-stages.json', {'profiler_sha256': digest(profiler.read_bytes()), 'rows': stages})

    # Re-read final objects. The original timing inventory was made before repeats.
    audits, dwarfs = [], []
    for row in rows:
        if not row.get('samples_ms'):
            continue
        a = inventory(Path(row['commands']['keel'][-1]))
        b = inventory(Path(row['commands']['clang-cc1as'][-1]))
        audits.append({'name': row['name'], 'parity': compare(a, b),
                       'final_sha256': {engine: digest(Path(cmd[-1]).read_bytes()) for engine, cmd in row['commands'].items()},
                       'matches_initial_inventory': {engine: digest(Path(cmd[-1]).read_bytes()) ==
                           json.loads((work / 'objects' / row['name'] / (engine + '.inventory.json')).read_text())['object_sha256']
                           for engine, cmd in row['commands'].items()}})
        if row['target'] != 'arm64' or row['group'] not in ('real', 'generated', 'generated-debug'):
            continue
        for engine in ('keel', 'clang-cc1as'):
            obj = row['commands'][engine][-1]
            for flag in ('--verify', '--debug-line', '--eh-frame'):
                result = invoke(['dwarfdump', flag, obj])
                path = work / 'objects' / row['name'] / (engine + flag + '.json')
                save(path, result)
                dwarfs.append({'name': row['name'], 'engine': engine, 'flag': flag,
                               'returncode': result['returncode'], 'evidence': str(path)})
    save(work / 'final-audit.json', audits)
    save(work / 'dwarf-audit.json', dwarfs)

    # Branch overflow without allocating a multi-gigabyte padding region.
    source = work / 'inputs/probe-x86-branch-overflow.s'
    source.write_text('.intel_syntax noprefix\n.text\nprobe: jmp probe+4294967296\n')
    template = next(r for r in rows if r['name'] == 'x86-small')
    branch_probe = []
    for engine, original in template['commands'].items():
        cmd = list(original)
        cmd[cmd.index(template['source'])] = str(source)
        cmd[-1] = str(work / ('probe-x86-branch-overflow-' + engine + '.o'))
        branch_probe.append({'engine': engine, 'command': cmd, **invoke(cmd)})
    save(work / 'x86-branch-overflow.json', branch_probe)

    # A sampling observation can identify hot code, but cannot assign the whole
    # cross-implementation gap to a particular algorithm. Never mix into timings.
    source = work / 'inputs/profile-arm64.s'
    mixed = (work / 'inputs/arm64-mixed-500000.s').read_text()
    source.write_text(mixed * 4)
    row = next(r for r in rows if r['name'] == 'arm64-mixed-500000')
    cmd = list(row['commands']['clang-cc1as'])
    cmd[cmd.index(row['source'])] = str(source)
    cmd[-1] = str(work / 'profile-clang.o')
    with (work / 'profile-clang.stdout').open('w') as stdout, (work / 'profile-clang.stderr').open('w') as stderr:
        child = subprocess.Popen(cmd, stdout=stdout, stderr=stderr)
        sample_cmd = ['/usr/bin/sample', str(child.pid), '1', '1', '-file', str(work / 'clang-sample.txt')]
        outcome = invoke(sample_cmd)
        status = child.wait(timeout=60)
    save(work / 'clang-profile.json', {'command': cmd, 'source_sha256': digest(source.read_bytes()),
         'sample_command': sample_cmd, 'sample_result': outcome, 'clang_returncode': status,
         'note': 'One 1-second, 1-ms sampling profile of 2,000,000 metadata-free AArch64 instructions; untimed explanatory evidence only.'})
    print('supplement complete', flush=True)


if __name__ == '__main__':
    main()
