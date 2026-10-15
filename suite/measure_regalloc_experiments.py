#!/usr/bin/env python3
"""Paired backend-only timings for saved defaults, new defaults and spill splitting.

Build profilers normally and with backend-profile,experimental-regalloc for the
split experiment. Run serially with no builds, tests or runtime benchmarks.
"""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import random
import statistics
import subprocess


def bootstrap(values):
    rng = random.Random(20261015)
    medians = sorted(statistics.median(rng.choices(values, k=len(values))) for _ in range(5000))
    return {'median_ratio': statistics.median(values), 'bootstrap_95pct': [medians[125], medians[4874]]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ['baseline_profiler', 'candidate_profiler', 'experimental_profiler', 'suite_cache', 'output']:
        parser.add_argument('--' + name.replace('_', '-'), type=Path, required=True)
    parser.add_argument('--opt', action='store_true', help='optimize IR outside backend timing')
    parser.add_argument('--pairs', type=int, default=5)
    parser.add_argument('--rounds', type=int, default=3)
    args = parser.parse_args()
    if min(args.pairs, args.rounds) < 3:
        parser.error('use at least three pairs and internal rounds')
    modes = {
        'old-fast': (args.baseline_profiler.resolve(), ['--regalloc-fast']),
        'fast': (args.candidate_profiler.resolve(), ['--regalloc-fast']),
        'old-full': (args.baseline_profiler.resolve(), []),
        'full': (args.candidate_profiler.resolve(), []),
        'split': (args.experimental_profiler.resolve(), ['--regalloc-split']),
    }
    if args.opt:
        modes = {name: (binary, flags + ['--opt']) for name, (binary, flags) in modes.items()}
    report = {'host': platform.platform(), 'optimized': args.opt, 'pairs': args.pairs, 'rounds': args.rounds,
              'method': 'Cached preprocessed C, baseline selection; IR optimization controlled by the optimized field and runs outside timing. Backend-only serial paired processes; five modes in seeded randomized order per source. Each process discards two warmups and alternates text/structured and inactive/active profiling. Results below use structured output with inactive collection, summing per-source medians in each pair. Parsing, IR verification and I/O excluded from backend timing. Compiler-time and runtime phases must run separately. Split uses a feature-enabled experimental binary; normal Fast/Full use a default binary. Ratios use paired summed medians; percentile bootstrap over process pairs. No LLVM comparison.',
              'modes': {name: {'binary': str(binary), 'sha256': hashlib.sha256(binary.read_bytes()).hexdigest(), 'flags': flags} for name, (binary, flags) in modes.items()},
              'cases': {}}
    sources = {'lua': sorted((args.suite_cache / 'lua/pre').glob('*.i')),
               'quickjs-core': [args.suite_cache / 'quickjs/pre/quickjs.i'],
               'sqlite-core': [args.suite_cache / 'sqlite/pre/sqlite3.i']}
    rng = random.Random(20261015)
    for case, paths in sources.items():
        if not paths:
            raise ValueError(f'no cached sources for {case}')
        rows = []
        totals = {mode: [] for mode in modes}
        allocations = {mode: [] for mode in modes}
        scans = {mode: [] for mode in modes}
        liveness = {mode: [] for mode in modes}
        for pair in range(args.pairs):
            times = {mode: 0. for mode in modes}
            alloc = times.copy()
            scan = times.copy()
            live = times.copy()
            for source in paths:
                order = list(modes)
                rng.shuffle(order)
                for mode in order:
                    binary, flags = modes[mode]
                    result = subprocess.run([str(binary), str(source), str(args.rounds), *flags], check=True, capture_output=True, text=True, timeout=180)
                    row = json.loads(result.stdout)
                    samples = [r for r in row['runs'] if r['mode'] == 'structured']
                    times[mode] += statistics.median(r['plain_codegen_ms'] for r in samples)
                    alloc[mode] += statistics.median(r['stages']['allocation_overhead']['inclusive_ms'] for r in samples)
                    scan[mode] += statistics.median(r['stages']['linear_scan']['self_ms'] for r in samples)
                    live[mode] += statistics.median(r['stages']['liveness']['self_ms'] for r in samples)
                    rows.append({'pair': pair, 'mode': mode, 'source': str(source), 'source_sha256': hashlib.sha256(source.read_bytes()).hexdigest(), 'data': row})
            for mode in modes:
                totals[mode].append(times[mode])
                allocations[mode].append(alloc[mode])
                scans[mode].append(scan[mode])
                liveness[mode].append(live[mode])
            print(case, pair + 1, times, flush=True)
        comparisons = {a + '/' + b: bootstrap([x / y for x, y in zip(totals[a], totals[b])]) for a, b in [('fast', 'old-fast'), ('full', 'old-full'), ('split', 'full')]}
        report['cases'][case] = {'backend_ms': totals, 'allocation_ms': allocations, 'scan_ms': scans, 'liveness_ms': liveness, 'comparisons': comparisons, 'samples': rows}
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report, indent=2) + '\n')
        print('summary', case, comparisons, flush=True)


if __name__ == '__main__':
    main()
