#!/usr/bin/env python3
"""Read-only statistics/report builder for validate_runtime_waves.py evidence."""
import argparse
import json
import math
from pathlib import Path
import random
import statistics as st

CONTRASTS = {'primary': ('after-default', 'before-default'),
             'all': ('after-all', 'before-all'),
             'rollout': ('after-all', 'after-default'),
             'baseline_all': ('before-all', 'before-default')}


def gm(xs):
    return math.exp(st.mean(math.log(x) for x in xs))


def aggregate(rows):
    out = {}
    for name in CONTRASTS:
        ratios = [r['contrasts'][name]['paired_ratios'] for r in rows]
        count = len(ratios[0]); assert all(len(r) == count for r in ratios)
        rng = random.Random(42); boots = []
        for _ in range(10000):
            indices = rng.choices(range(count), k=count)
            boots.append(gm([st.median([r[i] for i in indices]) for r in ratios]))
        boots.sort()
        block_ratios = [gm([r[i] for r in ratios]) for i in range(count)]
        block_median = st.median(block_ratios)
        out[name] = {'median': gm([st.median(r) for r in ratios]),
                     'ci95': [boots[249], boots[9749]],
                     'block_geomean_ratios': block_ratios,
                     'block_dispersion': {'median': block_median,
                         'mad': st.median(abs(r - block_median) for r in block_ratios),
                         'range': [min(block_ratios), max(block_ratios)]},
                     'method': 'Equal-workload geometric mean of paired median ratios; seed42 10000 common-block bootstrap resamples.'}
    return out


def result(r):
    lo, hi = r['ci95']
    return 'gain' if hi < 1 else 'regression' if lo > 1 else 'inconclusive'


def fmt(r):
    return f"{r['median']:.4f} [{r['ci95'][0]:.4f}, {r['ci95'][1]:.4f}]"


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--evidence', type=Path, required=True, help='directory with machost/ and linuxhost/ evidence')
    ap.add_argument('--output', type=Path, required=True)
    args = ap.parse_args()
    summary = {'ratio_direction': 'lower is faster; numerator/denominator recorded in contrasts',
               'contrasts': CONTRASTS, 'hosts': {}}
    lines = ['# Final runtime wave 1–4 validation', '',
             'Evidence only. Production source is unchanged. Compiler reports and AVX are off; scalar selection stays opt-in.', '',
             'Ratios below one favor the numerator. All samples, including warmups, are retained. '
             'Intervals are seeded 95% paired bootstrap intervals (10,000 resamples). '
             'Every configuration has two warmup and twelve measured complete blocks, with rotated/reversed order. '
             'No samples are removed. Statistical labels describe this campaign; small effects remain subject to host/background limits.', '']
    for host in ['machost', 'linuxhost']:
        root = args.evidence / host
        data = {n: json.loads((root / (n + '.json')).read_text()) for n in
                ['provenance', 'build', 'checks', 'runtime', 'compile', 'inspection']}
        assert len(data['checks']) == 76 and all(r['passed'] for r in data['checks'])
        data['aggregates'] = {app: aggregate([r for k, r in data['runtime'].items() if k.startswith(app + '/')])
                              for app in ['lua', 'quickjs', 'sqlite']}
        for row in data['runtime'].values():
            for c in row['contrasts'].values(): c['classification'] = result(c)
        summary['hosts'][host] = data
        lines += ['## ' + host.title(), '',
                  '| Workload | Before default ms | After default ms | Default after/before [95% CI] | all after/before | After all/default | Before all/default | Default finding |',
                  '|---|---:|---:|---|---|---|---|---|']
        for key, r in data['runtime'].items():
            lines.append(f"| {key} | {r['latency_ms']['before-default']['median']:.3f} | {r['latency_ms']['after-default']['median']:.3f} | " +
                         ' | '.join(fmt(r['contrasts'][n]) for n in CONTRASTS) + ' | ' + result(r['contrasts']['primary']) + ' |')
        lines += ['', 'Application aggregates use equal workload weights within each app; these are not usage-weighted product speedups.', '',
                  '| Application | Default after/before | all after/before | After all/default | Before all/default |', '|---|---|---|---|---|']
        for app, agg in data['aggregates'].items(): lines.append('| ' + app + ' | ' + ' | '.join(fmt(agg[n]) for n in CONTRASTS) + ' |')
        lines += ['', '### Runtime variation', '',
                  'Each entry is paired ratio median / MAD / min–max. These ranges retain every measured block.', '',
                  '| Workload | Default after/before | all after/before | After all/default | Before all/default |', '|---|---|---|---|---|']
        for key, r in data['runtime'].items():
            vals = [f"{r['contrasts'][n]['median']:.4f} / {r['contrasts'][n]['mad']:.4f} / {r['contrasts'][n]['range'][0]:.4f}–{r['contrasts'][n]['range'][1]:.4f}" for n in CONTRASTS]
            lines.append('| ' + key + ' | ' + ' | '.join(vals) + ' |')
        lines += ['', '### Normal CLI compilation cost', '',
                  'Milliseconds sum the same preprocessed TUs per app per block (33/9/2). '
                  'Ordinary release CLI `.i → assembly` includes startup, input read, C parsing/lowering, IR optimization, '
                  'backend selection/register allocation, assembly serialization/write and teardown. '
                  'It excludes preprocessing, system assembly and linking. No stage-specific instrumentation or opt-report timing is used. '
                  'These whole-CLI results cannot isolate frontend, optimizer or backend cost.', '',
                  '| Corpus | Before default ms | After default ms | Default after/before | all after/before | After all/default | Before all/default |',
                  '|---|---:|---:|---|---|---|---|']
        for app, r in data['compile'].items():
            lines.append(f"| {app} | {r['latency_ms']['before-default']['median']:.3f} | {r['latency_ms']['after-default']['median']:.3f} | " + ' | '.join(fmt(r['contrasts'][n]) for n in CONTRASTS) + ' |')
        lines += ['', '### Exact application sizes', '',
                  '| Config | App | Object text bytes | Object total bytes | Executable text bytes | Executable total bytes |', '|---|---|---:|---:|---:|---:|']
        for c, apps in data['build'].items():
            for app, d in apps.items():
                lines.append(f"| {c} | {app} | {d['object_text_bytes']} | {d['object_file_bytes']} | {d['binary']['text_bytes']} | {d['binary']['file_bytes']} |")
        lines += ['', '### Compiler identities', '']
        for v, c in data['provenance']['compilers'].items():
            lines += [f"* {v}: `{c['revision']}`; `{c['binary']}`; SHA-256 `{c['sha256']}`."]
        lines += ['']
    args.output.mkdir(parents=True, exist_ok=True)
    (args.output / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
    (args.output / 'metrics.md').write_text('\n'.join(lines) + '\n')


if __name__ == '__main__':
    main()
