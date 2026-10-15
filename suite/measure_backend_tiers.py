#!/usr/bin/env python3
"""Compare saved and candidate allocators on fixed preprocessed application inputs.

The profiler times frontend-free codegen. Compile-time and runtime phases must
run serially without builds/tests in the background. Build runtime executables
with --build-only first; --runtime-only reuses them. Baseline selection and
unoptimized IR are held fixed so this measures allocator differences alone.
"""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import random
import statistics
import subprocess
import time


def run(command, **kwargs):
    return subprocess.run(list(map(str, command)), check=True, capture_output=True, timeout=180, **kwargs)


def bootstrap(ratios):
    rng = random.Random(20261015)
    estimates = sorted(statistics.median(rng.choices(ratios, k=len(ratios))) for _ in range(5000))
    return {'median_ratio': statistics.median(ratios), 'bootstrap_95pct': [estimates[125], estimates[4874]]}


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--suite-cache', type=Path, required=True)
    p.add_argument('--baseline-compiler', type=Path, required=True)
    p.add_argument('--candidate-compiler', type=Path)
    p.add_argument('--baseline-profiler', type=Path)
    p.add_argument('--candidate-profiler', type=Path)
    p.add_argument('--work-dir', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--pairs', type=int, default=5)
    p.add_argument('--profile-rounds', type=int, default=3)
    p.add_argument('--runtime-rounds', type=int, default=7)
    p.add_argument('--opt', action='store_true', help='optimize IR before backend timing and in runtime builds')
    p.add_argument('--build-only', action='store_true')
    p.add_argument('--runtime-only', action='store_true')
    p.add_argument('--runtime-tier', choices=['both', 'full', 'fast'], default='both', help='restrict prebuilt runtime-only comparisons')
    p.add_argument('--profile-only', action='store_true')
    args = p.parse_args()
    if min(args.pairs, args.profile_rounds, args.runtime_rounds) < 3:
        p.error('use at least three pairs/profile/runtime rounds')
    root = Path(__file__).resolve().parent.parent
    args.work_dir.mkdir(parents=True, exist_ok=True)
    candidates = {'baseline-full': args.baseline_compiler.resolve(), 'baseline-fast': args.baseline_compiler.resolve()}
    if args.candidate_compiler:
        candidates.update({'candidate-full': args.candidate_compiler.resolve(), 'candidate-fast': args.candidate_compiler.resolve()})
    if args.runtime_tier != 'both':
        if not args.runtime_only:
            p.error('--runtime-tier requires --runtime-only')
        candidates = {name: binary for name, binary in candidates.items() if name.endswith('-' + args.runtime_tier)}
    report = {'host': platform.platform(),
              'cpu': (subprocess.check_output(['sysctl', '-n', 'machdep.cpu.brand_string'], text=True).strip() if platform.system() == 'Darwin' else platform.processor()), 'compiler_hashes': {name: hashlib.sha256(path.read_bytes()).hexdigest() for name, path in candidates.items()},
              'optimized': args.opt,
              'method': 'Fixed cached preprocessed C; baseline selection, identical IR optimization settings. Profiles exclude frontend, IR optimization, verification and file I/O. Balanced randomized process order; first two internal profile rounds discarded. Ratios use paired summed per-file medians; 5000 paired bootstrap resamples, seed 20261015. Runtime rounds are randomized, one warmup, correctness outputs must match. Application objects are direct AArch64 Mach-O. Normal-build runtime executables have no profiling instrumentation.',
              'profiler_hashes': {name: hashlib.sha256(path.read_bytes()).hexdigest() for name,path in [('baseline',args.baseline_profiler),('candidate',args.candidate_profiler)] if path},
              'builds': {}, 'profiles': {}, 'runtime': {}}
    rng = random.Random(20261015)
    if not args.runtime_only and not args.profile_only:
        for suite, binary, libs in [('lua', 'lua', ['m']), ('quickjs', 'qjs', ['m', 'pthread']), ('sqlite', 'sqlite3', ['m', 'pthread'])]:
            for name, compiler in candidates.items():
                directory = args.work_dir / name / suite
                directory.mkdir(parents=True, exist_ok=True)
                inputs = sorted((args.suite_cache / suite / 'pre').glob('*.i'))
                objects = []
                for source in inputs:
                    obj = directory / (source.stem + '.o')
                    command = [compiler, 'cc', source, '--emit-object', '--regalloc', name.split('-')[-1], '-o', obj]
                    if args.opt:
                        command.append('-O')
                    run(command)
                    objects.append(obj)
                executable = directory / binary
                run(['clang', *objects, *('-l' + lib for lib in libs), '-o', executable])
                report['builds'][name + '/' + suite] = {'files': len(inputs), 'object_bytes': sum(path.stat().st_size for path in objects),
                    'binary_bytes': executable.stat().st_size, 'sha256': hashlib.sha256(executable.read_bytes()).hexdigest(),
                    'source_hashes': {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in inputs}}
                print('built', name, suite, flush=True)
    if not args.runtime_only and not args.profile_only and args.candidate_compiler:
        comparisons = []
        for tier in ('full', 'fast'):
            for suite in ('lua', 'quickjs', 'sqlite'):
                before = args.work_dir / ('baseline-' + tier) / suite
                after = args.work_dir / ('candidate-' + tier) / suite
                for old in sorted(before.glob('*.o')):
                    new = after / old.name
                    if old.read_bytes() != new.read_bytes():
                        raise RuntimeError(f'output-preserving candidate changed {tier}/{suite}/{old.name}')
                    comparisons.append({'tier': tier, 'suite': suite, 'input': old.stem, 'identical': True})
        report['compatibility'] = {'matching_objects': len(comparisons), 'comparisons': comparisons}
        (args.work_dir / 'compatibility.json').write_text(json.dumps(report['compatibility'], indent=2) + '\n')
    if not args.build_only and not args.runtime_only:
        if not args.baseline_profiler or not args.candidate_profiler or not args.candidate_compiler:
            p.error('profile comparison requires both profilers and candidate compiler')
        cases = {'lua': sorted((args.suite_cache/'lua/pre').glob('*.i')),
                 'quickjs-core': [args.suite_cache/'quickjs/pre/quickjs.i'],
                 'sqlite-core': [args.suite_cache/'sqlite/pre/sqlite3.i']}
        for workload, sources in cases.items():
            totals = {name: [] for name in candidates}
            samples = []
            for pair in range(args.pairs):
                pair_totals = {name: 0. for name in candidates}
                for source in sources:
                    names = list(candidates)
                    rng.shuffle(names)
                    for name in names:
                        profiler = args.baseline_profiler if name.startswith('baseline') else args.candidate_profiler
                        command = [profiler.resolve(), source.resolve(), args.profile_rounds]
                        if args.opt:
                            command.append('--opt')
                        if name.endswith('fast'):
                            command.append('--regalloc-fast')
                        row = json.loads(run(command, text=True).stdout)
                        structured = [sample for sample in row['runs'] if sample['mode'] == 'structured']
                        pair_totals[name] += statistics.median(sample['plain_codegen_ms'] for sample in structured)
                        samples.append({'pair': pair, 'candidate': name, 'source': str(source),
                                        'source_sha256': hashlib.sha256(source.read_bytes()).hexdigest(), 'result': row})
                for name, value in pair_totals.items():
                    totals[name].append(value)
                print('profiled', workload, pair + 1, pair_totals, flush=True)
            comparisons = {}
            for numerator, denominator in [('candidate-full', 'baseline-full'), ('candidate-fast', 'baseline-fast'), ('candidate-fast', 'candidate-full')]:
                comparisons[numerator + '/' + denominator] = bootstrap([a/b for a,b in zip(totals[numerator], totals[denominator])])
            report['profiles'][workload] = {'summed_codegen_ms': totals, 'comparisons': comparisons, 'samples': samples}
    if not args.build_only and not args.profile_only:
        workloads = [(f'lua-{case}', 'lua', ['lua', str(root/'suite/lua/bench.lua'), case], None) for case in ['fib', 'sort', 'string', 'float']]
        workloads += [(f'quickjs-{case}', 'quickjs', ['qjs', '--stack-size', '4194304', str(root/'suite/quickjs/bench.js'), case], None) for case in ['fib', 'sort', 'string']]
        workloads += [('sqlite-queries', 'sqlite', ['sqlite3', ':memory:'], (root/'suite/sqlite/bench.sql').read_bytes())]
        for workload, suite, argv, stdin in workloads:
            samples = {name: [] for name in candidates}
            expected = None
            for iteration in range(args.runtime_rounds + 1):
                names = list(candidates)
                rng.shuffle(names)
                for name in names:
                    command = [str((args.work_dir/name/suite/argv[0]).resolve()), *argv[1:]]
                    start = time.perf_counter_ns()
                    result = run(command, input=stdin)
                    elapsed = (time.perf_counter_ns() - start) / 1e6
                    if expected is None:
                        expected = result.stdout
                    if result.stdout != expected:
                        raise RuntimeError(f'output mismatch: {workload} {name}')
                    if iteration:
                        samples[name].append(elapsed)
            comparisons = {a+'/'+b: bootstrap([x/y for x,y in zip(samples[a],samples[b])])
                           for a,b in [('candidate-full','baseline-full'),('candidate-fast','baseline-fast'),('candidate-fast','candidate-full')]
                           if a in candidates and b in candidates}
            report['runtime'][workload] = {'samples_ms': samples, 'comparisons': comparisons,
                                         'stdout_sha256': hashlib.sha256(expected).hexdigest()}
            print('runtime', workload, {name: statistics.median(values) for name,values in samples.items()}, flush=True)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + '\n')


if __name__ == '__main__':
    main()
