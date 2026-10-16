#!/usr/bin/env python3
"""Native AArch64 targeted checks. No downloads, compiler builds, or app runs."""
import argparse
import collections
import hashlib
import json
import pathlib
import platform
import random
import re
import shutil
import statistics
import subprocess
import sys
import time

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parent.parent
sys.path.insert(0, str(HERE.parent))
from profile_app_loops import text_bytes


def sha(path):
    return hashlib.sha256(pathlib.Path(path).read_bytes()).hexdigest()


def verify(identities):
    for path, digest in identities.items():
        if sha(path) != digest:
            raise RuntimeError(f'identity changed: {path}')


def command(argv, log=None):
    argv = list(map(str, argv))
    start = time.perf_counter()
    p = subprocess.run(argv, capture_output=True, timeout=120)
    if log is not None:
        log.append({'argv': argv, 'wall_seconds': time.perf_counter()-start,
                    'returncode': p.returncode, 'stderr': p.stderr.decode(errors='replace')})
    if p.returncode:
        raise RuntimeError(f'{argv}: exit {p.returncode}\n{p.stderr.decode(errors="replace")}')
    return p.stdout.decode()


def logical(word):
    """Decode ARM logical-immediate fields, independent of register allocation."""
    if word & 0x1f800000 != 0x12000000:
        return None
    width = 64 if word >> 31 else 32
    n, r, s = (word >> 22) & 1, (word >> 16) & 63, (word >> 10) & 63
    length = ((n << 6) | (~s & 63)).bit_length()-1
    if length < 1 or (width == 32 and n):
        return None
    size = 1 << length
    if size > width or (s & (size-1)) == size-1:
        return None
    ones = (1 << ((s & (size-1))+1))-1
    rotation = r & (size-1)
    lane = ((ones >> rotation) | (ones << (size-rotation))) & ((1 << size)-1)
    mask = sum(lane << shift for shift in range(0, width, size))
    return [['and', 'orr', 'eor', 'ands'][(word >> 29) & 3], width, hex(mask)]


def inspect(code, disassembly, case):
    moves = collections.Counter()
    immediates = []
    materializations = []
    for offset in range(0, len(code), 4):
        word = int.from_bytes(code[offset:offset+4], 'little')
        value = logical(word)
        if value:
            immediates.append(value)
        if word & 0x1f800000 == 0x12800000:
            op = {0: 'movn', 2: 'movz', 3: 'movk'}.get((word >> 29) & 3, 'reserved')
            moves[op] += 1
            materializations.append([op, 64 if word >> 31 else 32, (word >> 5) & 65535, ((word >> 21) & 3)*16])
    ops = collections.Counter()
    backedges = 0
    for line in disassembly.splitlines():
        m = re.match(r'\s*([0-9a-f]+)\s+([a-z][a-z0-9.]*)\s*(.*)', line)
        if not m:
            continue
        address, op, operands = m.groups()
        ops[op] += 1
        if op == 'b' or op.startswith('b.') or op in ('cbz', 'cbnz', 'tbz', 'tbnz'):
            target = re.search(r'0x([0-9a-f]+)\b', operands)
            if target and int(target[1], 16) < int(address, 16):
                backedges += 1
    expected = case.get('expected_logical')
    def present(pattern):
        return any(op == pattern[0] and width == pattern[1] and int(mask, 16) == int(pattern[2], 16)
                   for op, width, mask in immediates)
    shape = present(expected) if expected else None
    if case.get('expected_materialization'):
        shape = case['expected_materialization'] in materializations
    forbidden = case.get('forbidden_logical')
    forbidden_present = present(forbidden) if forbidden else False
    loads = sum(count for op, count in ops.items() if op.startswith(('ldr', 'ldp', 'ldur', 'ld1')))
    # Necessary evidence only. A human must still review the retained disassembly.
    eligible = case['id'] != 0 and loads > 0 and backedges > 0
    return {'text_bytes': len(code), 'instruction_words': len(code)//4,
            'op_counts': dict(ops), 'materialization_counts': dict(moves), 'materializations': materializations,
            'logical_immediates': immediates, 'expected_shape_present': shape,
            'forbidden_present': forbidden_present, 'loads': loads, 'backedges': backedges,
            'measurement_eligible': eligible,
            'eligibility_reason': 'load and backward branch retained; manual review required' if eligible else
                                  'call-floor control' if case['id'] == 0 else 'payload loop/load absent: invalid for intended measurement'}


def sample(binary, case, n, repeat, seed, mode='random', poison=False):
    argv = [binary, case['id'], n, repeat, seed, mode]
    if poison:
        argv.append('poison')
    start = time.perf_counter()
    row = json.loads(command(argv))
    outer = time.perf_counter()-start
    seconds = row['seconds']
    if not 0 < seconds <= outer + 0.005 or row['n'] != n or row['repeat'] != repeat:
        raise RuntimeError('invalid timer or work count')
    if int(row['checksum'], 16) != (int(row['single'], 16)*repeat) % (1 << 64):
        raise RuntimeError('invalid checksum record')
    return dict(row, process_seconds=outer)


def interval(ratios):
    rng = random.Random(1761)
    boot = sorted(statistics.median(rng.choices(ratios, k=len(ratios))) for _ in range(4000))
    return {'median_ratio': statistics.median(ratios), 'bootstrap_95_percent': [boot[99], boot[3899]],
            'pairs': len(ratios)}


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--config', type=pathlib.Path)
    p.add_argument('--work-dir', type=pathlib.Path)
    p.add_argument('--benchmark', action='store_true')
    p.add_argument('--cases', help='optional comma-separated functional subset; default all cases')
    p.add_argument('--timing-cases', default='and64,movn-negative')
    p.add_argument('--rounds', type=int, default=6)
    p.add_argument('--target-seconds', type=float, default=0.12)
    p.add_argument('--verify-report', type=pathlib.Path)
    args = p.parse_args()
    if args.verify_report:
        verify(json.loads(args.verify_report.read_text())['identities'])
        print('identities verified'); return
    if not args.config or not args.work_dir:
        p.error('--config and --work-dir required')
    if platform.machine() not in ('arm64', 'aarch64') or platform.system() != 'Darwin':
        p.error('initial native runner supports AArch64 macOS only')
    if args.rounds < 4 or args.rounds % 2 or not 0.1 <= args.target_seconds <= 0.25:
        p.error('use even rounds >=4 and target seconds in [0.1, 0.25]')
    work = args.work_dir.resolve()
    # Generated sources, objects, reports must stay under ignored artifact roots.
    if not any(work.is_relative_to(ROOT / base) for base in ('target', 'suite/reports')):
        p.error('work directory must be under this checkout target/ or suite/reports/')
    if work.exists():
        p.error('use a fresh work directory; existing evidence is never overwritten')
    config = json.loads(args.config.read_text())
    clang = pathlib.Path(config['clang']).resolve()
    variants = config['variants']
    names = [v['name'] for v in variants]
    if not variants or len(set(names)) != len(names) or 'clang-O2' in names:
        p.error('provide unique variants; clang-O2 is reserved')
    for v in variants:
        if not re.fullmatch('[a-zA-Z0-9_-]+', v['name']):
            p.error('invalid variant name')
        v['compiler'] = str(pathlib.Path(v['compiler']).resolve())
        for key in ('source_id', 'features', 'flags'):
            if key not in v:
                p.error(f'each variant needs explicit {key}')
    cases = json.loads((HERE/'cases.json').read_text())
    if args.cases:
        requested = set(args.cases.split(','))
        if requested - {c['name'] for c in cases}:
            p.error('unknown functional case')
        cases = [c for c in cases if c['name'] in requested or c['id'] == 0]
    selected = args.timing_cases.split(',')
    if args.benchmark and set(selected) - {c['name'] for c in cases if c['id']}:
        p.error('unknown timing case')
    variants.append({'name': 'clang-O2', 'compiler': str(clang), 'source_id': command([clang, '--version']),
                     'features': [], 'flags': ['-O2', '-march=armv8-a', '-ffp-contract=off']})
    sources = [HERE/f for f in ('kernels.c', 'driver.c', 'cases.json', 'run.py', 'test_runner.py')]
    identities = {str(f.resolve()): sha(f) for f in [*sources, args.config, clang,
                    HERE.parent/'profile_app_loops.py', *[pathlib.Path(v['compiler']) for v in variants]]}
    work.mkdir(parents=True)
    report = {'schema': 1, 'complete': False, 'identities': identities, 'config': config,
              'harness_revision': command(['git', '-C', ROOT, 'rev-parse', 'HEAD']).strip(),
              'harness_dirty': command(['git', '-C', ROOT, 'status', '--porcelain', '--', HERE, HERE.parent/'README.md']),
              'host': platform.platform(), 'target': command([clang, '-dumpmachine']).strip(),
              'input_generator': 'LCG64, seed 1761; functional seeds 0,1,1761 plus edge words',
              'timer': 'CLOCK_MONOTONIC kernel-call loop; setup/oracle/printing excluded; no subtraction',
              'commands': [], 'builds': {}, 'timing': {}, 'raw_samples': [], 'activity': []}
    def save():
        (work/'report.json').write_text(json.dumps(report, indent=2)+'\n')
    def run(argv):
        return command(argv, report['commands'])
    def freeze(path):
        identities[str(path)] = sha(path)
    save()
    try:
        driver = work/'driver.o'
        run([clang, '-O2', '-std=c11', '-march=armv8-a', '-ffp-contract=off', '-c', HERE/'driver.c', '-o', driver])
        freeze(driver)
        for case in cases:
            name = case['name']
            source = work/(name+'.i')
            run([clang, '-E', '-P', '-std=c11', '-DCASE='+str(case['id']), HERE/'kernels.c', '-o', source])
            freeze(source)
            report['builds'][name] = {}
            for v in variants:
                stem = work/(name+'-'+v['name'])
                asm, obj = stem.with_suffix('.s'), stem.with_suffix('.o')
                if v['name'] == 'clang-O2':
                    run([v['compiler'], *v['flags'], '-S', source, '-o', asm])
                else:
                    run([v['compiler'], 'cc', *v['flags'], source, '-o', asm])
                run([clang, '-c', asm, '-o', obj])
                run([clang, driver, obj, '-o', stem])
                disassembly = run(['/usr/bin/otool', '-tvV', obj])
                stem.with_suffix('.disasm').write_text(disassembly)
                shape = inspect(text_bytes(obj), disassembly, case)
                if shape['forbidden_present']:
                    raise RuntimeError(f'{name}: forbidden logical immediate')
                if case['feature'] in v['features'] and shape['expected_shape_present'] is False:
                    raise RuntimeError(f'{name}/{v["name"]}: requested feature shape absent')
                for f in (asm, obj, stem):
                    freeze(f)
                checks = []
                for n in (0, 1, 7, 257):
                    for seed, mode in ((0, 'random'), (1, 'random'), (1761, 'random'), (1, 'edges')):
                        checks.append(sample(stem, case, n, 10000, seed, mode))
                report['builds'][name][v['name']] = {'binary': str(stem), 'shape': shape,
                    'object_bytes': obj.stat().st_size, 'binary_text_bytes': len(text_bytes(stem)),
                    'functional': checks}
            print('checked', name, flush=True)
            save()
        verify(identities)
        # Deliberate checksum corruption must fail even when the payload is correct.
        control = cases[0]
        binary = report['builds'][control['name']][variants[0]['name']]['binary']
        try:
            sample(binary, control, 1, 10000, 1761, poison=True)
        except RuntimeError as error:
            if 'exit 4' not in str(error):
                raise
            report['checksum_negative_control'] = 'PASS (driver exit 4)'
        else:
            raise RuntimeError('negative checksum control accepted')
        if args.benchmark:
            print('all builds complete; starting timing (no compiler builds from here)', flush=True)
            def activity(label):
                report['activity'].append({'label': label, 'time_ns': time.time_ns(),
                    'ps': command(['ps', '-axo', 'pid,pcpu,comm'])})
            activity('before')
            for case in [c for c in cases if c['name'] in selected or not c['id']]:
                name = case['name']
                detail = report['timing'][name] = {}
                for n in (1, 4096):
                    if case['id'] and not all(report['builds'][name][v['name']]['shape']['measurement_eligible'] for v in variants):
                        detail[str(n)] = {'invalid': 'payload loop/load absent in at least one variant'}
                        continue
                    # Common repeat count, calibrated to the slowest variant. Retain probes.
                    repeat = 10000
                    probes = []
                    for _ in range(5):
                        observed = []
                        for v in variants:
                            binary = report['builds'][name][v['name']]['binary']
                            row = sample(binary, case, n, repeat, 1761)
                            probes.append(dict(row, variant=v['name']))
                            observed.append(row['seconds'])
                        slowest = max(observed)
                        if slowest >= args.target_seconds*0.8 or repeat == 1000000000:
                            break
                        repeat = min(1000000000, max(repeat+1, int(repeat*args.target_seconds/slowest)))
                    entry = detail[str(n)] = {'calibration': probes, 'repeat': repeat,
                                              'scales': {}, 'intervals': {}}
                    # Half/full repeat counts diagnose scaling and timer/driver floor.
                    # Each block uses one forward/reverse order, giving balanced pairs.
                    for scale in (1, 2):
                        count = max(1, repeat//2) if scale == 1 else repeat
                        pairs = []
                        warmups = []
                        for v in variants:
                            warmups.append(dict(sample(report['builds'][name][v['name']]['binary'], case, n, count, 1761), variant=v['name']))
                        for r in range(args.rounds):
                            verify(identities)
                            order = variants if r % 2 == 0 else list(reversed(variants))
                            block = {}
                            for v in order:
                                # All variants use the same executable pathname; copy is untimed.
                                runtime = work/'runtime'
                                source_binary = report['builds'][name][v['name']]['binary']
                                shutil.copyfile(source_binary, runtime); runtime.chmod(0o755)
                                if sha(runtime) != identities[source_binary]:
                                    raise RuntimeError('runtime binary identity changed')
                                row = sample(runtime, case, n, count, 1761)
                                block[v['name']] = row['seconds']
                                report['raw_samples'].append(dict(row, case=name, variant=v['name'],
                                    round=r, scale=scale, order=[x['name'] for x in order]))
                            pairs.append(block)
                        base = variants[0]['name']
                        entry['scales'][str(scale)] = {'repeat': count, 'pairs': pairs, 'warmups': warmups,
                            'comparisons_over_'+base: {v['name']: interval([b[v['name']]/b[base] for b in pairs])
                                                     for v in variants[1:]}}
                        save()
                    entry['full_over_half'] = {v['name']: statistics.median([b[v['name']] for b in entry['scales']['2']['pairs']]) /
                                              statistics.median([b[v['name']] for b in entry['scales']['1']['pairs']]) for v in variants}
                activity('after '+name); save()
        verify(identities)
        report['complete'] = True
        save()
        print('PASS:', work/'report.json')
    except Exception as error:
        report['failure'] = str(error)
        save()
        raise


if __name__ == '__main__':
    main()
