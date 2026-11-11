"""Native column-reduction screen with strict, shared numerical/alias oracles."""
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import random
import re
import shutil
import statistics
import subprocess


def padded(text, placement, linux):
    if not placement:
        return text
    def insert(match):
        quantum, count = (16, 4) if linux else (4, 16)
        fill = hashlib.sha256(f'{placement}:{match.group(1)}'.encode()).digest()[0] % count
        return f'.space {fill * quantum}\n' + match.group(0)
    # Match the aligned function header, including private helpers without .globl.
    result = re.sub(r'^[ \t]*\.p2align[^\n]*\n(?:[ \t]*\.type[^\n]*\n)?"?(\w+)"?:\n(?=[^\n]*_begin:)',
                    insert, text, flags=re.M)
    if result == text:
        raise RuntimeError('placement did not find function symbols')
    return result


def run_columns(args):
    linux = platform.system() == 'Linux' and platform.machine().lower() in ('x86_64', 'amd64')
    mac = platform.system() == 'Darwin' and platform.machine().lower() in ('arm64', 'aarch64')
    if not (linux or mac):
        raise SystemExit('--columns requires x86-64 Linux or AArch64 macOS')
    if args.rounds < 1 or not 1 <= args.placements <= 16 or args.matrix_elements < 1 or any(
            n < 1 or n > 2048 for n in args.dimensions):
        raise SystemExit('positive rounds/elements, dimensions 1..2048 and placements 1..16 required')
    if linux:
        if args.cpu == args.controller_cpu:
            raise SystemExit('benchmark and controller CPUs must differ')
        os.sched_setaffinity(0, {args.controller_cpu})
    source = Path(__file__).resolve().parent
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    keel = args.keel.resolve()
    clang = shutil.which(args.clang) or args.clang
    report = dict(complete=False, host=platform.platform(), machine=platform.machine(),
                  rounds=args.rounds, placements=args.placements, warmup=1,
                  metric='cycles:u' if linux else 'cycles (time -l, user and kernel)',
                  cpu=args.cpu if linux else None, controller_cpu=args.controller_cpu if linux else None,
                  keel_sha256=hashlib.sha256(keel.read_bytes()).hexdigest(),
                  sources={n: hashlib.sha256((source / n).read_bytes()).hexdigest()
                           for n in ('columns.c', 'columns_driver.c', 'columns.py', 'run.py')},
                  commands=[], samples=[], summary=[])
    def save():
        (out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    def run(argv):
        argv = list(map(str, argv))
        proc = subprocess.run(argv, capture_output=True, text=True, timeout=600)
        report['commands'].append(dict(argv=argv, returncode=proc.returncode,
                                       stdout=proc.stdout, stderr=proc.stderr))
        save()
        if proc.returncode:
            raise RuntimeError(f'{argv}\n{proc.stdout}\n{proc.stderr}')
        return proc.stdout
    run([clang, '-O2', '-fno-fast-math', '-ffp-contract=off', '-c', source / 'columns_driver.c', '-o', out / 'driver.o'])
    run([keel, 'cc', '-O3', '--opt-disable', 'loop-column-reduce', '-Dcol64=baseline64', '-Dcol32=baseline32',
         '--emit-object', source / 'columns.c', '-o', out / 'reference.o'])
    # Both sides receive the same scalar preparation and Full pipeline.
    # The only difference is the explicit column-reduction pass between them.
    run([keel, 'cc', '-O0', '--emit-ir', source / 'columns.c', '-o', out / 'raw.ir'])
    prefix = 'simplify-cfg,canonicalize,stack-promote,instcombine,dce'
    run([keel, 'opt', '-p', prefix, out / 'raw.ir', '-o', out / 'scalar.ir'])
    run([keel, 'opt', '-p', 'loop-column-reduce', out / 'scalar.ir', '-o', out / 'tiled.ir'])
    if (out / 'tiled.ir').read_text().count('func @__keel_columns_') != 2:
        raise RuntimeError('column transform did not reach both kernels')
    images = {}
    for name in ['scalar', 'tiled']:
        asm = out / (name + '.s')
        run([keel, 'compile', '-O3', out / (name + '.ir'), '-o', asm])
        for place in range(args.placements):
            a = out / f'{name}-{place}.s'
            a.write_text(padded(asm.read_text(), place, linux))
            obj = a.with_suffix('.o')
            run([clang, '-c', a, '-o', obj])
            binary = out / f'{name.ljust(7, "_")}-{place}'
            run([clang, *(['-no-pie'] if linux else []), out / 'driver.o', out / 'reference.o', obj, '-o', binary])
            run([binary, 'check'])
            images[name, place] = binary
    for place in range(args.placements):
        binary = out / f'control-{place}'
        shutil.copy2(images['scalar', place], binary)
        images['control', place] = binary
    print('All numerical, guard-page, overlap and adjacency checks passed.', flush=True)
    rng = random.Random(59)
    total = args.matrix_elements
    for n in args.dimensions:
        repeats = min(100000, max(1, total // (n * n)))
        for kind in [0, 1]:
            expected = run([images['scalar', 0], kind, n, repeats])
            for round_index in range(-1, args.rounds):
                places = list(range(args.placements))
                rng.shuffle(places)
                for place in places:
                    order = ['scalar', 'tiled', 'control']
                    rng.shuffle(order)
                    for name in order + order[::-1]:
                        counters = out / 'counters.txt'
                        prefix = (['perf', 'stat', '-x,', '-e', 'cycles:u,instructions:u', '-o', str(counters), '--',
                                   'taskset', '-c', str(args.cpu)] if linux else
                                  ['/usr/bin/time', '-l', '-o', str(counters)])
                        proc = subprocess.run(prefix + [str(images[name, place]), str(kind), str(n), str(repeats)],
                                              capture_output=True, text=True, timeout=120)
                        if proc.returncode or proc.stdout != expected or proc.stderr:
                            raise RuntimeError(f'{name} n={n}: checksum/status mismatch: {proc}')
                        if round_index >= 0:
                            text = counters.read_text()
                            cycles = int(re.search(r'^(\d+),,cycles:u', text, re.M)[1]) if linux else int(
                                re.search(r'(\d+)\s+cycles elapsed', text)[1])
                            instructions = int(re.search(r'^(\d+),,instructions:u', text, re.M)[1]) if linux else int(
                                re.search(r'(\d+)\s+instructions retired', text)[1])
                            report['samples'].append(dict(n=n, kernel='f32' if kind else 'f64', placement=place,
                                                          round=round_index, variant=name, cycles=cycles,
                                                          instructions=instructions, repeats=repeats))
            for name in ['tiled', 'control']:
                ratios = []
                for place in range(args.placements):
                    per_round = []
                    for r in range(args.rounds):
                        def block(label):
                            return sum(x['cycles'] for x in report['samples'] if
                                       (x['n'], x['kernel'], x['placement'], x['round'], x['variant']) ==
                                       (n, 'f32' if kind else 'f64', place, r, label))
                        per_round.append(block(name) / block('scalar'))
                    ratios.append(statistics.median(per_round))
                report['summary'].append(dict(n=n, kernel='f32' if kind else 'f64', variant=name,
                                              ratio=math.exp(statistics.mean(map(math.log, ratios))), placements=ratios))
            save()
            print(n, 'f32' if kind else 'f64', [(x['variant'], round(100 * (x['ratio'] - 1), 2))
                  for x in report['summary'] if x['n'] == n and x['kernel'] == ('f32' if kind else 'f64')], flush=True)
    report['complete'] = True
    save()
