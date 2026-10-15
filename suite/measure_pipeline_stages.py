#!/usr/bin/env python3
"""Compare prebuilt profile_stages examples in randomized paired rounds.

Build/copy the baseline before editing, then rebuild the candidate with:
  cargo build --release -p keel-asm --example profile_stages
Inputs are scaling workloads derived from checked-in IR fixtures. Reading,
verification, and prerequisite stages happen outside each stage's timer;
allocation and destruction of each measured result are included.
"""
import argparse
import hashlib
import json
import platform
import random
import re
import statistics
import subprocess
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--baseline', type=Path, required=True)
    parser.add_argument('--candidate', type=Path, required=True)
    parser.add_argument('--work-dir', type=Path, default=Path('target/pipeline-stages'))
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--pairs', type=int, default=12)
    parser.add_argument('--iterations', type=int, default=5)
    args = parser.parse_args()
    if args.pairs < 1 or args.iterations < 1:
        parser.error('pairs and iterations must be positive')
    args.work_dir.mkdir(parents=True, exist_ok=True)
    root = Path(__file__).resolve().parent.parent
    rng = random.Random(847188)
    report = {'platform': platform.platform(), 'pairs': args.pairs,
              'iterations': args.iterations, 'seed': 847188,
              'binaries': {name: {'path': str(path), 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}
                           for name, path in [('baseline', args.baseline), ('candidate', args.candidate)]},
              'workloads': []}
    for fixture, repeats in [('arith', 150), ('regalloc_loops', 120), ('regalloc_pressure', 100)]:
        original = root / f'crates/keel-codegen/tests/programs/{fixture}.keel'
        source = re.sub(r'^;.*\n', '', original.read_text(), flags=re.M)
        source = '\n'.join(re.sub(r'@(\w+)', lambda m: f'@{m[1]}_{i}', source) for i in range(repeats))
        path = args.work_dir / f'{fixture}.keel'
        path.write_text(source)
        samples = {'baseline': [], 'candidate': []}
        for _ in range(args.pairs):
            order = list(samples)
            rng.shuffle(order)
            for name in order:
                binary = getattr(args, name).resolve()
                samples[name].append(json.loads(subprocess.check_output([str(binary), str(path), str(args.iterations)], text=True)))
        summary = {}
        for stage in ['parse_ms', 'codegen_ms', 'assemble_ms']:
            medians = {name: statistics.median(x for sample in runs for x in sample[stage]) for name, runs in samples.items()}
            ratios = [statistics.median(c[stage]) / statistics.median(b[stage]) for b, c in zip(samples['baseline'], samples['candidate'])]
            bootstrap_rng = random.Random(847188)
            bootstrap = sorted(statistics.median(bootstrap_rng.choices(ratios, k=len(ratios))) for _ in range(5000))
            summary[stage] = {**medians, 'paired_median_ratio': statistics.median(ratios),
                              'paired_ratio_bootstrap_95pct': [bootstrap[125], bootstrap[4874]]}
        report['workloads'].append({'fixture': str(original.relative_to(root)), 'repeats': repeats,
                                    'source_sha256': hashlib.sha256(source.encode()).hexdigest(),
                                    'summary': summary, 'samples': samples})
        print(f'{fixture}: ' + ', '.join(f'{stage}: {100 * (1 - stat["paired_median_ratio"]):.1f}% lower' for stage, stat in summary.items()), flush=True)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + '\n')


if __name__ == '__main__':
    main()
