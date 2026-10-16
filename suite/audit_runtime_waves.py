#!/usr/bin/env python3
"""Recompute retained block statistics and verify phase serialization."""
import argparse
import datetime
import json
import math
from pathlib import Path
import random
import statistics as st

CONTRASTS = {'primary': ('after-default', 'before-default'),
             'all': ('after-all', 'before-all'),
             'rollout': ('after-all', 'after-default'),
             'baseline_all': ('before-all', 'before-default')}


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('evidence', type=Path)
    b = ap.parse_args().evidence
    audit = {}; identities = {}; phases = []; ends = []
    for host in ['machost', 'linuxhost']:
        d = b / host
        p = json.loads((d / 'provenance.json').read_text())
        build = json.loads((d / 'build.json').read_text())
        identities[host] = {'compilers': p['compilers'], 'applications': {
            c: {a: r['binary'] for a, r in apps.items()} for c, apps in build.items()}}
        audit[host] = {}

        def when(phase):
            raw = json.loads((d / ('boundary-' + phase + '.json')).read_text())[0]['stdout'].strip()
            for fmt in ['%a %b %d %H:%M:%S %Z %Y', '%A, %B %d, %Y %p%I:%M:%S %Z']:
                try:
                    return datetime.datetime.strptime(raw, fmt)
                except ValueError:
                    pass
            raise ValueError(raw)

        for name in ['runtime', 'compile']:
            data = json.loads((d / (name + '.json')).read_text())
            count = 0
            for key, row in data.items():
                assert all(r['warmup'] == (r['block'] < 0) and -2 <= r['block'] < 12 for r in row['runs'])
                for config, samples in row['samples_ms'].items():
                    rebuilt = []
                    for i in range(12):
                        vals = [r['elapsed_ms'] for r in row['runs'] if r['config'] == config and r['block'] == i]
                        assert len(vals) == (1 if name == 'runtime' else {'lua': 33, 'quickjs': 9, 'sqlite': 2}[key])
                        rebuilt.append(sum(vals))
                    assert rebuilt == samples, (host, name, key, config)
                for contrast, (num, den) in CONTRASTS.items():
                    ratios = [x/y for x, y in zip(row['samples_ms'][num], row['samples_ms'][den])]
                    c = row['contrasts'][contrast]
                    assert c['paired_ratios'] == ratios and c['median'] == st.median(ratios)
                    assert c['mad'] == st.median(abs(x-st.median(ratios)) for x in ratios)
                    assert c['range'] == [min(ratios), max(ratios)]
                    rng = random.Random(42)
                    boots = sorted(st.median(rng.choices(ratios, k=12)) for _ in range(10000))
                    assert c['ci95'] == [boots[249], boots[9749]]
                count += len(row['runs'])
            assert count == (448 if name == 'runtime' else 2464)
            audit[host][name] = {'raw_runs': count, 'complete_blocks': 12, 'warmup_blocks': 2,
                                'recomputed_samples_ratios_median_mad_range_bootstrap': True}
            phases.append((when(name + '-start'), when(name + '-end'), host, name))
        for phase in ['build-end', 'check-end', 'inspect-end']:
            ends.append(when(phase))
        summary = json.loads((b / 'summary.json').read_text())['hosts'][host]
        runtime = json.loads((d / 'runtime.json').read_text())
        def gm(xs):
            return math.exp(st.mean(math.log(x) for x in xs))
        for app in ['lua', 'quickjs', 'sqlite']:
            rows = [r for k, r in runtime.items() if k.startswith(app + '/')]
            for contrast, (num, den) in CONTRASTS.items():
                ratios = [[x/y for x, y in zip(r['samples_ms'][num], r['samples_ms'][den])] for r in rows]
                actual = summary['aggregates'][app][contrast]
                assert actual['median'] == gm([st.median(r) for r in ratios])
                blocks = [gm([r[i] for r in ratios]) for i in range(12)]
                assert actual['block_geomean_ratios'] == blocks
                assert actual['block_dispersion'] == {'median': st.median(blocks),
                    'mad': st.median(abs(r-st.median(blocks)) for r in blocks), 'range': [min(blocks), max(blocks)]}
                rng = random.Random(42); boots = []
                for _ in range(10000):
                    indices = rng.choices(range(12), k=12)
                    boots.append(gm([st.median([r[i] for i in indices]) for r in ratios]))
                boots.sort()
                assert actual['ci95'] == [boots[249], boots[9749]]
        audit[host]['application_aggregates_recomputed'] = True
    phases.sort()
    assert max(ends) < phases[0][0]
    for x, y in zip(phases, phases[1:]):
        assert x[1] <= y[0], (x, y)
    audit['phases'] = [{'host': h, 'phase': p, 'start_utc': str(a), 'end_utc': str(z)} for a, z, h, p in phases]
    audit['all_build_check_inspect_before_timing'] = True
    audit['timing_serialized_between_hosts'] = True
    (b / 'statistics-audit.json').write_text(json.dumps(audit, indent=2) + '\n')
    (b / 'identities.json').write_text(json.dumps(identities, indent=2) + '\n')
    print(json.dumps(audit, indent=2))


if __name__ == '__main__':
    main()
