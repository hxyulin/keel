#!/usr/bin/env python3
"""Serial paired compile-boundary measurements on the suite's preprocessed C.

Build release keel and its profile_compile example first. No builds or other
benchmarks should run concurrently. Native object output is measured on macOS.
"""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import random
import shutil
import statistics
import subprocess
import time


def run(argv):
    p = subprocess.run(list(map(str, argv)), capture_output=True)
    if p.returncode:
        raise RuntimeError(f'{argv}: {p.stderr.decode(errors="replace")}')
    return p.stdout


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--compiler', type=Path, default=Path('target/release/keel'))
    ap.add_argument('--profiler', type=Path, default=Path('target/release/examples/profile_compile'))
    ap.add_argument('--work-dir', type=Path, required=True)
    ap.add_argument('--output', type=Path, required=True)
    ap.add_argument('--pairs', type=int, default=7)
    args = ap.parse_args()
    if args.pairs < 3:
        ap.error('at least three pairs are required')
    work = args.work_dir.resolve()
    work.mkdir(parents=True, exist_ok=True)
    compiler, profiler = work / 'keel-fixed', work / 'profile-compile-fixed'
    for src, dst in [(args.compiler.resolve(), compiler), (args.profiler.resolve(), profiler)]:
        if src != dst:
            shutil.copy2(src, dst)
    mac = platform.system() == 'Darwin'
    inputs = [(suite, p.resolve()) for suite in ['lua', 'quickjs', 'sqlite']
              for p in sorted((Path('target/suite') / suite / 'pre').glob('*.i'))]
    assert len(inputs) == 44, len(inputs)
    modes = ['clang-S-O0', 'clang-c-O0', 'clang-c-O2', 'keel-S', 'keel-O-S',
             'keel-clang-object', 'keel-O-clang-object', 'clang-asm-only']
    if mac:
        modes += ['keel-direct-object', 'keel-O-direct-object', 'keel-own-asm-object']
    paths = {}
    for mode in modes:
        for suite, src in inputs:
            d = work / mode / suite
            d.mkdir(parents=True, exist_ok=True)
            paths[mode, suite, str(src)] = (d / (src.stem + '.s'), d / (src.stem + '.o'))

    def commands(mode, suite, src):
        asm, obj = paths[mode, suite, str(src)]
        if mode.startswith('clang-') and mode != 'clang-asm-only':
            return [['clang', '-w', '-O2' if mode.endswith('O2') else '-O0',
                     '-S' if '-S-' in mode else '-c', src, '-o', asm if '-S-' in mode else obj]]
        if mode == 'clang-asm-only':
            source = paths['keel-S', suite, str(src)][0]
            return [['clang', '-c', source, '-o', obj]]
        cmd = [compiler, 'cc'] + (['-O'] if mode.startswith('keel-O-') else []) + [src]
        if 'direct' in mode:
            return [cmd + ['--emit-object', '-o', obj]]
        first = cmd + ['-o', asm]
        if mode.endswith('-S'):
            return [first]
        if mode == 'keel-own-asm-object':
            return [first, [compiler, 'asm', asm, '-o', obj]]
        return [first, ['clang', '-c', asm, '-o', obj]]

    report = {'commit': run(['git', 'rev-parse', 'HEAD']).decode().strip(),
              'platform': platform.platform(), 'host': platform.node(),
              'compiler_sha256': sha(compiler), 'profiler_sha256': sha(profiler),
              'script_sha256': sha(Path(__file__)), 'pairs': args.pairs,
              'clang': run(['clang', '--version']).decode().strip(),
              'method': '44 preprocessed files, no preprocessing/linking; immutable binaries; serial warm-cache batches, seeded randomized mode order in each paired round, one full warmup. Assembly-only uses saved unoptimized Keel assembly. Library stages use two warmups and five samples per input; source read/startup/write excluded, destruction included. Whole-object equality checks compare direct output with Keel text assembly, not Clang output.',
              'commands': {}, 'inputs': [{'suite': s, 'path': str(p), 'sha256': sha(p)} for s, p in inputs],
              'cli_samples_ms': {s: {m: [] for m in modes} for s in ['lua', 'quickjs', 'sqlite']},
              'stages': {}, 'object_equality': []}
    # Warm all paths and check complete native objects before collecting timing.
    for mode in modes:
        report['commands'][mode] = []
        for suite, src in inputs:
            cs = commands(mode, suite, src)
            report['commands'][mode].extend([list(map(str, c)) for c in cs])
            for cmd in cs:
                run(cmd)
    if mac:
        for suite, src in inputs:
            direct = paths['keel-direct-object', suite, str(src)][1]
            text = paths['keel-own-asm-object', suite, str(src)][1]
            assert direct.read_bytes() == text.read_bytes(), src
            # Optimized text round trip is validation-only, not another timed mode.
            asm = paths['keel-O-S', suite, str(src)][0]
            obj = work / 'optimized-text-check.o'
            run([compiler, 'asm', asm, '-o', obj])
            optimized = paths['keel-O-direct-object', suite, str(src)][1]
            assert obj.read_bytes() == optimized.read_bytes(), src
            report['object_equality'].append({'input': str(src), 'plain_sha256': sha(direct),
                                              'optimized_sha256': sha(optimized)})
    rng = random.Random(20261015)
    for round_number in range(args.pairs + 1):
        order = modes.copy()
        rng.shuffle(order)
        for mode in order:
            for suite in ['lua', 'quickjs', 'sqlite']:
                start = time.perf_counter_ns()
                for s, src in inputs:
                    if s == suite:
                        for cmd in commands(mode, suite, src):
                            run(cmd)
                elapsed = (time.perf_counter_ns() - start) / 1e6
                if round_number:
                    report['cli_samples_ms'][suite][mode].append(elapsed)
        print(f'CLI round {round_number}/{args.pairs}', flush=True)
    for tier, output in [('none', 'text'), ('full', 'text')] + ([('none', 'object'), ('full', 'object')] if mac else []):
        key = tier + '-' + output
        report['stages'][key] = []
        for suite in ['lua', 'quickjs', 'sqlite']:
            selected = [src for s, src in inputs if s == suite]
            rows = run([profiler, '5', tier, output, *selected]).decode().splitlines()
            report['stages'][key].extend([dict(json.loads(row), suite=suite) for row in rows])
        print('Library stages:', key, flush=True)
    report['cli_median_ms'] = {s: {m: statistics.median(v) for m, v in samples.items()}
                               for s, samples in report['cli_samples_ms'].items()}
    report['cli_batch_median_ms'] = {m: statistics.median([sum(report['cli_samples_ms'][s][m][i]
                                           for s in report['cli_samples_ms']) for i in range(args.pairs)]) for m in modes}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report['cli_batch_median_ms'], indent=2))


if __name__ == '__main__':
    main()
