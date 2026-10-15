"""Paired frontend comparison with cross-variant Debug IR equality.

Build and freeze variants first. Counter binaries are measured separately.
"""
import argparse
import hashlib
import json
import math
import pathlib
import platform
import random
import statistics
import subprocess

from measure_preprocessor import interval


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(argv):
    result = subprocess.run(argv, capture_output=True, text=True)
    if result.returncode:
        raise RuntimeError(f'{argv}: {result.stderr}')
    return result.stdout


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--profile', type=pathlib.Path, required=True)
    p.add_argument('--variant', action='append', required=True)
    p.add_argument('--output', type=pathlib.Path, required=True)
    p.add_argument('--work-dir', type=pathlib.Path, required=True)
    p.add_argument('--rounds', type=int, default=9)
    p.add_argument('--resume', action='store_true', help='Extend an existing complete paired run, retaining every sample')
    args = p.parse_args()
    binaries = {name: pathlib.Path(path).resolve() for name, path in (s.split('=', 1) for s in args.variant)}
    assert 'baseline' in binaries and args.rounds >= 5
    source = json.loads(args.profile.read_text())
    inputs = {app: [r['path'] for r in rows] for app, rows in source['inputs'].items()}
    for rows in source['inputs'].values():
        for row in rows:
            assert sha(pathlib.Path(row['path'])) == row['sha256']
    work = args.work_dir.resolve()
    report = {'git_head': subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
              'host': platform.platform(), 'rounds': args.rounds, 'seed': 20261015,
              'method': 'Serial warm resident-input public compile and staged library calls. Each process warms/verifies both APIs and checks exact Debug IR equality. Cross-variant IR snapshots are also byte-identical in normal and debug modes. Applications shuffled and variant order rotates each pair; source order rotates across rounds. No preprocessing, optimizer, backend, input reads, verification or returned-module destruction in timed regions. Paired ratios use same-round public compile totals, with bootstrap intervals; no other-machine claim.',
              'input_profile_sha256': sha(args.profile), 'script_sha256': sha(pathlib.Path(__file__)),
              'sources': {str(path): sha(path) for path in sorted(pathlib.Path('crates/keel-c/src').glob('*.rs'))},
              'inputs': source['inputs'], 'binaries': {k: {'path': str(v), 'sha256': sha(v)} for k, v in binaries.items()},
              'validation': {}, 'samples': [], 'summaries': {}, 'aggregate': {}}
    completed = 0
    if args.resume:
        previous = json.loads(args.output.read_text())
        for key in ['git_head', 'input_profile_sha256', 'sources', 'inputs', 'binaries', 'seed']:
            assert previous[key] == report[key], f'resume changed {key}'
        completed = previous['rounds']
        assert completed < args.rounds
        assert len(previous['samples']) == completed * len(inputs) * 4 * len(binaries)
        for index in range(completed):
            keys = {(s['app'], s['debug'], s['mode'], s['variant']) for s in previous['samples'] if s['round'] == index}
            assert len(keys) == len(inputs) * 4 * len(binaries)
        report['samples'] = previous['samples']
        report['measurement_sessions'] = previous.get('measurement_sessions', [
            {'first_round': 0, 'rounds': completed, 'script_sha256': previous['script_sha256']}]) + [
            {'first_round': completed, 'rounds': args.rounds - completed, 'script_sha256': report['script_sha256']}]
        report['extension_note'] = 'Extended complete paired run without discarding any samples; initial intervals were widened by large latency spikes in both variants.'
    for app, paths in inputs.items():
        for debug in [False, True]:
            hashes = {}
            for name, binary in binaries.items():
                out = work / name / app / ('debug' if debug else 'normal')
                command = [str(binary), 'prepare', '1', str(out), *paths] + (['--debug'] if debug else [])
                run(command)
                hashes[name] = {path.name: sha(path) for path in sorted(out.glob('*.ir'))}
                assert len(hashes[name]) == len(paths)
                assert hashes[name] == hashes['baseline'], f'IR changed: {app}/{debug}/{name}'
            report['validation'][f'{app}:{debug}'] = {'files': len(paths), 'byte_identical_IR': True,
                'IR_verified': True, 'output_sha256': hashes['baseline']}
            print('Verified', app, 'debug' if debug else 'normal', len(paths), 'files', flush=True)
    rng = random.Random(report['seed'])
    names = list(binaries)
    for round_index in range(args.rounds):
        jobs = [(app, debug, mode) for app in inputs for debug in [False, True] for mode in ['baseline', 'staged']]
        rng.shuffle(jobs)
        if round_index < completed:
            continue
        for app, debug, mode in jobs:
            paths = inputs[app]
            shift = round_index % len(paths)
            paths = paths[shift:] + paths[:shift]
            variant_shift = round_index % len(names)
            for name in names[variant_shift:] + names[:variant_shift]:
                command = [str(binaries[name]), mode, '1', *paths] + (['--debug'] if debug else [])
                rows = [json.loads(line) for line in run(command).splitlines()]
                report['samples'].append({'variant': name, 'app': app, 'debug': debug,
                    'mode': mode, 'round': round_index, 'command': command,
                    'total_ms': sum(x['total_ms'] for x in rows), 'files': rows})
        args.output.write_text(json.dumps(report, indent=2) + '\n')
        print('Paired frontend round', round_index + 1, '/', args.rounds, flush=True)
    for app in inputs:
        for debug in [False, True]:
            selected = [x for x in report['samples'] if x['app'] == app and x['debug'] == debug]
            summary = {'variants': {}, 'comparisons': {}}
            times = {}
            for name in names:
                groups = {mode: [x for x in selected if x['variant'] == name and x['mode'] == mode] for mode in ['baseline', 'staged']}
                times[name] = [x['total_ms'] for x in groups['baseline']]
                summary['variants'][name] = {'compile_median_ms': statistics.median(times[name]),
                    'stages_median_ms': {k: statistics.median(sum(f[k] for f in row['files']) for row in groups['staged']) for k in ['lex_ms', 'parse_ms', 'irgen_ms', 'cleanup_ms']},
                    'staged_to_public_ratio': statistics.median(x['total_ms']/y['total_ms'] for x,y in zip(groups['staged'],groups['baseline']))}
            for name in names[1:]:
                ratios = [a/b for a,b in zip(times[name], times['baseline'])]
                summary['comparisons'][name] = {'paired_ratios': ratios, 'median_ratio': statistics.median(ratios),
                    'ratio_ci95': interval(ratios, rng), 'speedup': 1/statistics.median(ratios)}
            report['summaries'][f'{app}:{debug}'] = summary
    for debug in [False, True]:
        for name in names[1:]:
            ratios = [report['summaries'][f'{app}:{debug}']['comparisons'][name]['paired_ratios'] for app in inputs]
            center = math.exp(statistics.mean(math.log(statistics.median(r)) for r in ratios))
            draws = sorted(math.exp(statistics.mean(math.log(statistics.median(rng.choices(r,k=len(r)))) for r in ratios)) for _ in range(10000))
            report['aggregate'][f'{name}:{debug}'] = {'equal_application_geomean_ratio': center, 'speedup': 1/center, 'ratio_ci95': [draws[250],draws[9749]]}
    for name,binary in binaries.items():
        assert sha(binary) == report['binaries'][name]['sha256']
    args.output.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({'aggregate': report['aggregate'], 'summaries': report['summaries']}, indent=2))


if __name__ == '__main__':
    main()
