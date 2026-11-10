"""Ordered-reduction mode of run.py; AArch64 macOS assembly screening only."""
import hashlib
import json
import math
from pathlib import Path
import platform
import random
import re
import shutil
import statistics
import subprocess


def padded(text, placement):
    """Change function offsets identically across variants without executing fill."""
    if not placement:
        return text
    def insert(match):
        name = match.group(1)
        digest = hashlib.sha256(f'{placement}:{name}'.encode()).digest()
        return f'.space {(digest[0] % 16) * 4}\n' + match.group(0)
    return re.sub(r'^\s*\.globl\s+(_\w+)[^\n]*', insert, text, flags=re.M)


def run_ordered(args):
    if platform.system() != 'Darwin' or platform.machine().lower() not in ('arm64', 'aarch64'):
        raise SystemExit('--ordered requires AArch64 macOS (/usr/bin/time -l counters)')
    if not 1 <= args.placements <= 16 or args.rounds < 1 or args.iterations < 1 or any(
            n < 1 or n > 1048576 for n in args.sizes):
        raise SystemExit('positive rounds/iterations, sizes 1..1048576 and placements 1..16 required')
    source = Path(__file__).resolve().parent
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    keel = args.keel.resolve()
    clang = shutil.which(args.clang) or args.clang
    report = dict(complete=False, host=platform.platform(), machine=platform.machine(),
                  metric='whole-process cycles and instructions, /usr/bin/time -l',
                  semantics='strict left-to-right additions; no fast math',
                  placements=args.placements, rounds=args.rounds, warmup=1,
                  sources={n: hashlib.sha256((source / n).read_bytes()).hexdigest()
                           for n in ('ordered.c', 'ordered_driver.c', 'ordered_m5.S', 'ordered.py', 'run.py')},
                  keel_sha256=hashlib.sha256(keel.read_bytes()).hexdigest(),
                  commands=[], samples=[], summary=[])

    def save():
        (out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')

    def command(argv):
        argv = list(map(str, argv))
        proc = subprocess.run(argv, capture_output=True, text=True, timeout=600)
        report['commands'].append(dict(argv=argv, returncode=proc.returncode,
                                       stdout=proc.stdout, stderr=proc.stderr))
        save()
        if proc.returncode:
            raise RuntimeError(f'{argv}\n{proc.stdout}\n{proc.stderr}')
        return proc.stdout

    report['clang_version'] = command([clang, '--version'])
    strict = ['-fno-fast-math', '-ffp-contract=off']
    command([clang, '-O2', *strict, '-c', source / 'ordered_driver.c', '-o', out / 'driver.o'])
    command([keel, 'cc', '-O0', '--emit-ir', source / 'ordered.c', '-o', out / 'raw.ir'])
    prefix = 'simplify-cfg,canonicalize,stack-promote,instcombine,dce'
    # Both sides receive identical scalar preparation followed by full -O3.
    # Only the opt-in loop-batch invocation differs.
    for label in ('scalar', 'batch', 'batch-scalar'):
        pipeline = prefix + ({'batch': ',loop-batch', 'batch-scalar': ',loop-batch-scalar'}.get(label, ''))
        command([keel, 'opt', '-p', pipeline, out / 'raw.ir', '-o', out / (label + '.ir')])
        command([keel, 'compile', '-O3', out / (label + '.ir'), '-o', out / (label + '.s')])
    ir = (out / 'batch.ir').read_text()
    if not all(op in ir for op in ('load.f64x2', 'load.f32x4', 'vfabs')):
        raise RuntimeError('batching did not reach the expected kernels')
    command([clang, '-O3', *strict, '-S', source / 'ordered.c', '-o', out / 'clang.s'])
    for name, packed in (('hand-scalar', 0), ('hand-neon', 1)):
        command([clang, '-E', '-P', f'-DPACKED={packed}', source / 'ordered_m5.S', '-o', out / (name + '.s')])
    names = ['scalar', 'batch', 'batch-scalar', 'clang', 'hand-scalar', 'hand-neon']
    images = {}
    for placement in range(args.placements):
        for name in names:
            asm = out / f'{name}-{placement}.s'
            asm.write_text(padded((out / (name + '.s')).read_text(), placement))
            obj = out / f'{name}-{placement}.o'
            command([clang, '-c', asm, '-o', obj])
            binary = out / f'{name}-{placement}'
            command([clang, out / 'driver.o', obj, '-o', binary])
            command([binary, 'check'])
            images[name, placement] = binary
        control = out / f'control-{placement}'
        shutil.copyfile(images['scalar', placement], control)
        control.chmod(0o755)
        images['control', placement] = control
    report['control'] = 'byte-identical copy of scalar executable at each placement'
    print('All variants passed scalar-oracle and guard-page checks.', flush=True)
    rng = random.Random(17)
    labels = names + ['control']
    total = args.iterations * max(args.sizes)
    cases = [(kind, n) for kind in range(4) for n in args.sizes]
    # Warm up each workload and retain mirrored blocks for each placement.
    for kind, n in cases:
        repeats = max(1, total // n)
        if repeats > 100000000:
            raise SystemExit('too many short-kernel calls; reduce iterations')
        expected = command([images['scalar', 0], kind, n, repeats])
        for round_index in range(-1, args.rounds):
            placements = list(range(args.placements))
            rng.shuffle(placements)
            for placement in placements:
                order = labels.copy()
                rng.shuffle(order)
                for label in order + order[::-1]:
                    counters = out / 'counters.txt'
                    binary = images[label, placement]
                    proc = subprocess.run(['/usr/bin/time', '-l', '-o', str(counters),
                                           str(binary), str(kind), str(n), str(repeats)],
                                          capture_output=True, text=True, timeout=120)
                    if proc.returncode or proc.stdout != expected or proc.stderr:
                        raise RuntimeError(f'{binary}: checksum/status mismatch: {proc}')
                    if round_index >= 0:
                        text = counters.read_text()
                        metric = {key: int(re.search(r'(\d+)\s+' + key, text)[1])
                                  for key in ('cycles elapsed', 'instructions retired')}
                        report['samples'].append(dict(kernel=kind, n=n, repeats=repeats,
                                                      round=round_index, placement=placement,
                                                      variant=label, **metric))
        save()
        print(f'timed {("sum64", "abs64", "sum32", "abs32")[kind]} n={n}', flush=True)
    for kind, n in cases:
        for label in labels:
            ratios = []
            instruction_ratios = []
            for placement in range(args.placements):
                per_round = []
                per_instructions = []
                for r in range(args.rounds):
                    def block(variant, metric):
                        return sum(s[metric] for s in report['samples'] if
                                   (s['kernel'], s['n'], s['placement'], s['round'], s['variant']) ==
                                   (kind, n, placement, r, variant))
                    per_round.append(block(label, 'cycles elapsed') / block('scalar', 'cycles elapsed'))
                    per_instructions.append(block(label, 'instructions retired') /
                                            block('scalar', 'instructions retired'))
                ratios.append(statistics.median(per_round))
                instruction_ratios.append(statistics.median(per_instructions))
            row = dict(kernel=('sum64', 'abs64', 'sum32', 'abs32')[kind], n=n, variant=label,
                       cycles_ratio=math.exp(statistics.mean(map(math.log, ratios))),
                       placements=ratios,
                       instructions_ratio=math.exp(statistics.mean(map(math.log, instruction_ratios))))
            report['summary'].append(row)
            print(json.dumps(row), flush=True)
    report['complete'] = True
    save()
