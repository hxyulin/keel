#!/usr/bin/env python3
"""Replay the bounded memory corpus with two linkers at identical output paths.

Uses streaming_memory's per-child wait4 measurement, advisory lock, timeout and
RSS watchdog. No input generation or hashing is included in the timed interval.
"""
import argparse
import json
from pathlib import Path
import statistics
import subprocess
from types import SimpleNamespace

from streaming_memory import Experiment, M, digest


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--baseline', type=Path, required=True)
    p.add_argument('--candidate', type=Path, required=True)
    p.add_argument('--corpus', type=Path, required=True, help='Existing streaming_memory raw.json')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--repeats', type=int, default=3)
    args = p.parse_args()
    assert args.repeats > 0
    args.out = args.out.resolve()
    baseline, candidate = args.baseline.resolve(), args.candidate.resolve()
    source = json.loads(args.corpus.read_text())
    cases = {}
    groups = {'macho-link-few', 'macho-link-many', 'macho-link-debug-no-map',
              'macho-link-no-debug-control', 'cached-workload-link',
              'macho-symbols-relocations', 'elf-link-few', 'macho-link-bss'}
    for row in source['records']:
        if row['exit'] == 0 and row['group'] in groups:
            cases.setdefault(row['name'], row)
    exp = Experiment(SimpleNamespace(out=args.out, keel=candidate,
                                    source_commit='working tree; see source.patch', phase='primary'))
    exp.meta['baseline'] = {'path': str(baseline), 'sha256': digest(baseline)}
    exp.meta['corpus_sha256'] = digest(args.corpus)
    summaries = []
    for name, row in cases.items():
        inputs = [Path(x) for x in row['inputs']]
        assert [digest(x) for x in dict.fromkeys(inputs)] == row['input_sha256']
        output = args.out / (name + '.exe')
        hashes = []
        for repeat in range(args.repeats):
            # Alternate execution order to reduce systematic order bias.
            variants = [('baseline', baseline), ('candidate', candidate)]
            if repeat % 2:
                variants.reverse()
            for variant, binary in variants:
                command = [str(binary), *row['command'][1:]]
                command[-1] = str(output)
                assert exp.measure(name + '-' + variant, command, inputs, output,
                                   variant, row['scale']), name
                hashes.append(exp.records[-1]['output_sha256'])
        assert len(set(hashes)) == 1, f'{name}: output mismatch'
        if row['structure']['format'] == 'Mach-O64':
            subprocess.run(['codesign', '--verify', '--strict', output], check=True,
                           capture_output=True, timeout=10)
            subprocess.run([output], check=True, capture_output=True, timeout=10)
        data = {'case': name, 'input_MiB': row['input_bytes']/M,
                'output_identical': True, 'sha256': hashes[0]}
        for variant in ('baseline', 'candidate'):
            records = [r for r in exp.records if r['name'] == name + '-' + variant]
            data[variant] = {'rss_MiB': statistics.median(r['peak_rss_bytes']/M for r in records),
                             'seconds': statistics.median(r['elapsed_seconds'] for r in records)}
        data['rss_reduction_percent'] = 100*(1-data['candidate']['rss_MiB']/data['baseline']['rss_MiB'])
        summaries.append(data)
        (args.out/'summary.json').write_text(json.dumps(summaries, indent=2)+'\n')
    assert digest(candidate) == exp.meta['binary_sha256'], 'candidate changed during replay'
    assert digest(baseline) == exp.meta['baseline']['sha256'], 'baseline changed during replay'
    for r in summaries:
        print(r['case'], f"{r['baseline']['rss_MiB']:.1f} -> {r['candidate']['rss_MiB']:.1f} MiB",
              f"({r['rss_reduction_percent']:.1f}% reduction)")


if __name__ == '__main__':
    main()
