#!/usr/bin/env python3
"""Compare an experimental spill policy with a frozen Full allocator.

Run build, profile, and runtime phases separately; do not run other builds or
benchmarks concurrently. Inputs are the existing preprocessed application cache.
"""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import random
import statistics
import subprocess

from measure_backend_tiers import bootstrap


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(argv):
    result = subprocess.run(list(map(str, argv)), capture_output=True, timeout=180)
    if result.returncode:
        raise RuntimeError(f'{list(map(str, argv))} failed ({result.returncode}):\n{result.stderr.decode(errors="replace")}')
    return result


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('phase', choices=['build', 'profile'])
    p.add_argument('--baseline', type=Path, required=True)
    p.add_argument('--candidate', type=Path, required=True)
    p.add_argument('--tier', default='weighted', choices=['full', 'fast', 'weighted', 'split', 'local'])
    p.add_argument('--baseline-tier', default='full', choices=['full', 'fast', 'weighted', 'split', 'local'])
    p.add_argument('--selection-all', action='store_true', help='profile full instruction selection')
    p.add_argument('--cases', nargs='+', choices=['lua', 'quickjs-core', 'sqlite-core'], help='restrict profile cases')
    p.add_argument('--suites', nargs='+', choices=['lua', 'quickjs', 'sqlite'], default=['lua', 'quickjs', 'sqlite'], help='application build suites')
    p.add_argument('--remat', choices=['none','constants','stack','all','pressure'], default='none', help='candidate rematerialization mode')
    p.add_argument('--suite-cache', type=Path, required=True)
    p.add_argument('--work-dir', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--opt', action='store_true')
    p.add_argument('--pairs', type=int, default=5)
    p.add_argument('--rounds', type=int, default=3)
    args = p.parse_args()
    if min(args.pairs, args.rounds) < 3:
        p.error('use at least three process pairs and internal rounds')
    binaries = {'baseline-full': args.baseline.resolve(), 'candidate-full': args.candidate.resolve()}
    report = {'host': platform.platform(), 'optimized': args.opt, 'phase': args.phase,
              'candidate_remat':args.remat, 'candidate_policy': args.tier, 'baseline_policy': args.baseline_tier, 'selection_all': args.selection_all, 'pairs': args.pairs, 'rounds': args.rounds,
              'binaries': {k: {'path': str(v), 'sha256': digest(v)} for k, v in binaries.items()},
              'method': 'Frozen preprocessed sources; identical selection and IR optimization settings. Backend-only serial randomized process pairs; two internal warmups excluded by profiler. Per-process structured medians summed over files. Paired percentile bootstrap 95% intervals, 5000 resamples. Parsing, verification, optimization and I/O excluded. No LLVM comparison.',
              'cases': {}}
    if args.phase == 'build' and (args.cases or args.selection_all):
        p.error('--cases and --selection-all apply only to profiling')
    args.work_dir.mkdir(parents=True, exist_ok=True)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    rng = random.Random(20261015)
    if args.phase == 'build':
        for suite, executable, libs in [('lua', 'lua', ['m']), ('quickjs', 'qjs', ['m', 'pthread']), ('sqlite', 'sqlite3', ['m', 'pthread'])]:
            if suite not in args.suites:
                continue
            for name, compiler in binaries.items():
                directory = args.work_dir / name / suite
                directory.mkdir(parents=True, exist_ok=True)
                paths = sorted((args.suite_cache / suite / 'pre').glob('*.i'))
                if not paths:
                    raise ValueError(f'no sources for {suite}')
                objects = []
                for source in paths:
                    obj = directory / (source.stem + '.o')
                    tier = args.baseline_tier if name.startswith('baseline') else args.tier
                    remat_flags = [] if name.startswith('baseline') or args.remat == 'none' else ['--remat', args.remat]
                    if platform.system() == 'Darwin':
                        run([compiler, 'cc', source, '--emit-object', '--regalloc', tier,
                             '-o', obj, *(['-O'] if args.opt else []), *remat_flags])
                    else:
                        assembly = directory / (source.stem + '.s')
                        run([compiler, 'cc', source, '--regalloc', tier,
                             '-o', assembly, *(['-O'] if args.opt else []), *remat_flags])
                        run(['clang', '-c', assembly, '-o', obj])
                    objects.append(obj)
                exe = directory / executable
                run(['clang', *objects, *('-l' + lib for lib in libs), '-o', exe])
                report['cases'][name + '/' + suite] = {'objects': len(objects), 'object_bytes': sum(o.stat().st_size for o in objects),
                    'executable_bytes': exe.stat().st_size, 'sha256': digest(exe), 'source_hashes': {str(s): digest(s) for s in paths}}
                print('built', name, suite, flush=True)
                args.output.write_text(json.dumps(report, indent=2) + '\n')
    else:
        sources = {'lua': sorted((args.suite_cache / 'lua/pre').glob('*.i')),
                   'quickjs-core': [args.suite_cache / 'quickjs/pre/quickjs.i'],
                   'sqlite-core': [args.suite_cache / 'sqlite/pre/sqlite3.i']}
        if args.cases:
            sources = {name: sources[name] for name in args.cases}
        for case, paths in sources.items():
            if not paths:
                raise ValueError(f'no sources for {case}')
            totals = {k: [] for k in binaries}
            allocation = {k: [] for k in binaries}
            samples = []
            for pair in range(args.pairs):
                times = dict.fromkeys(binaries, 0.)
                alloc = times.copy()
                for source in paths:
                    names = list(binaries)
                    rng.shuffle(names)
                    for name in names:
                        tier = args.baseline_tier if name.startswith('baseline') else args.tier
                        remat_flags = [] if name.startswith('baseline') or args.remat == 'none' else ['--remat-' + args.remat]
                        flags = remat_flags + ([] if tier == 'full' else ['--regalloc-' + tier]) + (['--opt'] if args.opt else []) + (['--selection-all'] if args.selection_all else [])
                        row = json.loads(run([binaries[name], source, args.rounds, *flags]).stdout)
                        runs = [r for r in row['runs'] if r['mode'] == 'structured']
                        times[name] += statistics.median(r['plain_codegen_ms'] for r in runs)
                        alloc[name] += statistics.median(r['stages']['allocation_overhead']['inclusive_ms'] for r in runs)
                        samples.append({'pair': pair, 'variant': name, 'source': str(source), 'source_sha256': digest(source), 'data': row})
                for name in binaries:
                    totals[name].append(times[name])
                    allocation[name].append(alloc[name])
                print(case, pair + 1, times, flush=True)
            comparison = bootstrap([a / b for a, b in zip(totals['candidate-full'], totals['baseline-full'])])
            report['cases'][case] = {'backend_ms': totals, 'allocation_ms': allocation, 'comparison': comparison, 'samples': samples}
            args.output.write_text(json.dumps(report, indent=2) + '\n')
            print(case, comparison, flush=True)


if __name__ == '__main__':
    main()
