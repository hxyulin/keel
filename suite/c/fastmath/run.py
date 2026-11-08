#!/usr/bin/env python3
"""Build, check, and time fast-math baselines with a shared strict driver."""
import argparse
import hashlib
import json
import platform
from pathlib import Path
import random
import shutil
import statistics
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--keel', required=True, type=Path)
    parser.add_argument('--clang', default='clang')
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--sizes', type=int, nargs='+', default=[4099, 262147])
    parser.add_argument('--iterations', type=int, default=1000)
    parser.add_argument('--rounds', type=int, default=5)
    args = parser.parse_args()
    if args.rounds < 1 or not 1 <= args.iterations <= 1000000 or any(
            n < 1 or n > 1048576 for n in args.sizes):
        parser.error('positive rounds, sizes 1..1048576, iterations 1..1000000 required')
    if platform.machine().lower() not in ('arm64', 'aarch64', 'x86_64', 'amd64'):
        parser.error('requires native AArch64 or x86-64')
    source = Path(__file__).resolve().parent
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    clang = str(Path(shutil.which(args.clang) or args.clang).resolve())
    keel = str(args.keel.resolve())
    report = {'complete': False, 'host': platform.platform(), 'machine': platform.machine(),
              'sources': {p.name: hashlib.sha256(p.read_bytes()).hexdigest()
                          for p in (source / n for n in ('scalar.c', 'simd.c', 'driver.c', 'run.py'))},
              'commands': [], 'samples': [], 'summary': [],
              'keel_fast_math': 'unsupported; Keel rows use strict arithmetic'}

    def save():
        (out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')

    def run(argv):
        argv = list(map(str, argv))
        result = subprocess.run(argv, capture_output=True, text=True, timeout=600)
        report['commands'].append({'argv': argv, 'returncode': result.returncode,
                                   'stdout': result.stdout, 'stderr': result.stderr})
        save()
        if result.returncode:
            raise RuntimeError(f'{argv}:\n{result.stderr}')
        return result.stdout

    report['clang_version'] = run([clang, '--version'])
    report['keel_binary_sha256'] = hashlib.sha256(Path(keel).read_bytes()).hexdigest()
    # No LTO: the common driver cannot fold away or inline the kernels.
    strict = ['-fno-fast-math', '-ffp-contract=off']
    run([clang, '-O2', *strict, '-c', source / 'driver.c', '-o', out / 'driver.o'])
    configs = {
        'llvm-strict': ('scalar.c', [clang, '-O3', *strict]),
        'llvm-fast': ('scalar.c', [clang, '-O3', '-ffast-math']),
        'hand-strict': ('simd.c', [clang, '-O3', *strict]),
        'hand-fast': ('simd.c', [clang, '-O3', '-ffast-math', '-DFAST_APPROX']),
        'keel': ('scalar.c', [keel, 'cc']),
        'keel-O': ('scalar.c', [keel, 'cc', '-O']),
    }
    for name, (file, command) in configs.items():
        obj = out / (name + '.o')
        if name.startswith('keel'):
            # Match the suite's Clang preprocessor boundary and preserve assembly.
            run([clang, '-E', '-P', source / file, '-o', out / 'scalar.i'])
            asm = out / (name + '.s')
            run([*command, out / 'scalar.i', '-o', asm])
            run([clang, '-c', asm, '-o', obj])
        else:
            asm = out / (name + '.s')
            run([*command, '-S', source / file, '-o', asm])
            run([clang, '-c', asm, '-o', obj])
        # Linking never uses -ffast-math: avoid process-wide FTZ/DAZ startup code.
        run([clang, out / 'driver.o', obj, '-lm', '-o', out / name])
    # Check all short tails and an exact SIMD block before collecting samples.
    for name in configs:
        for n in (1, 3, 4, 7, 8, 17, *args.sizes):
            run([out / name, n, 1])
    rng = random.Random(0)
    for round_index in range(args.rounds):
        order = list(configs)
        rng.shuffle(order)
        for n in args.sizes:
            for name in order:
                for line in run([out / name, n, args.iterations]).splitlines():
                    row = json.loads(line)
                    row.update(config=name, round=round_index)
                    report['samples'].append(row)
    for n in args.sizes:
        for kernel in ('division', 'normalize', 'dot'):
            for name in configs:
                rows = [r for r in report['samples'] if
                        (r['n'], r['kernel'], r['config']) == (n, kernel, name)]
                ns = [r['seconds'] * 1e9 / r['iterations'] / n for r in rows]
                summary = dict(n=n, kernel=kernel, config=name,
                               median_ns_per_element=statistics.median(ns),
                               min_ns_per_element=min(ns), max_ns_per_element=max(ns),
                               max_scaled_error=max(r['max_scaled_error'] for r in rows))
                report['summary'].append(summary)
                print(json.dumps(summary))
    report['complete'] = True
    save()


if __name__ == '__main__':
    main()
