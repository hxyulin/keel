"""Summarize paired arena/cache/MMAP ablations and storage measurements."""
import argparse
import hashlib
import json
import math
import pathlib
import random
import statistics


def summary(values, rng):
    center = statistics.median(values)
    draws = sorted(statistics.median(rng.choices(values, k=len(values))) for _ in range(10000))
    return {'median_ratio': center, 'ratio_ci95': [draws[250], draws[9749]], 'speedup': 1 / center, 'paired_ratios': values}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--timing', type=pathlib.Path, required=True)
    parser.add_argument('--storage', type=pathlib.Path, required=True)
    parser.add_argument('--output', type=pathlib.Path, required=True)
    args = parser.parse_args()
    p = json.loads(args.timing.read_text())
    storage = json.loads(args.storage.read_text())
    rng = random.Random(20261015)
    comparisons = {'final_vs_previous': ('cache-mmap', 'baseline'),
                   'final_vs_original': ('cache-mmap', 'original'),
                   'arena_vs_previous': ('arena', 'baseline'),
                   'cache_vs_arena': ('cache-read', 'arena'),
                   'mmap_vs_read': ('cache-mmap', 'cache-read')}
    result = {'timing_sha256': hashlib.sha256(args.timing.read_bytes()).hexdigest(),
              'storage_sha256': hashlib.sha256(args.storage.read_bytes()).hexdigest(),
              'method': 'Same-round time ratios; median within each application. Bootstrap resamples pairs within each fixed application (10000 draws). Aggregates equally weight application median ratios geometrically; their intervals independently resample pairs within each application. Memory is full counting-harness peak RSS from a separate single process per application/version; allocation counts and RSS are not timed.',
              'applications': {}, 'aggregate': {}}
    for app, row in p['summaries'].items():
        times = {name: {r['round']: r['total_ms'] for r in p['samples'] if r['app'] == app and r['variant'] == name} for name in p['binaries']}
        entry = {'median_ms': row['median_ms'], 'comparisons': {}, 'storage': storage['applications'][app]}
        for name, (candidate, baseline) in comparisons.items():
            ratios = [times[candidate][i] / times[baseline][i] for i in range(p['rounds'])]
            entry['comparisons'][name] = summary(ratios, rng)
        result['applications'][app] = entry
    for name in comparisons:
        rows = [row['comparisons'][name]['paired_ratios'] for row in result['applications'].values()]
        center = math.exp(statistics.mean(math.log(statistics.median(r)) for r in rows))
        draws = sorted(math.exp(statistics.mean(math.log(statistics.median(rng.choices(r, k=len(r)))) for r in rows)) for _ in range(10000))
        result['aggregate'][name] = {'median_geomean_ratio': center, 'ratio_ci95': [draws[250], draws[9749]], 'speedup': 1 / center}
    args.output.write_text(json.dumps(result, indent=2) + '\n')
    for app, row in result['applications'].items():
        print(app, row['median_ms'])
        for name, c in row['comparisons'].items():
            print(' ', name, round(c['speedup'], 3), c['ratio_ci95'])
    print('aggregate', result['aggregate'])


if __name__ == '__main__':
    main()
