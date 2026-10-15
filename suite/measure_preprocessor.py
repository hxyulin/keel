"""Paired native-preprocessor comparison on a prior profile's exact raw corpus.

Build and archive every binary before running. Each app is warmed by the
example before measured work. The runner compares output byte-for-byte,
verifies generated IR, alternates variant order, and retains raw samples.
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


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def command(binary, mode, base, output):
    return [str(binary), mode, '1', base[3], base[4], str(output), *base[6:]]


def run(argv):
    result = subprocess.run(argv, capture_output=True, text=True)
    if result.returncode:
        raise RuntimeError(f'{argv}: {result.stderr}')
    return result.stdout


def interval(values, rng):
    draws = sorted(statistics.median(rng.choices(values, k=len(values))) for _ in range(10000))
    return [draws[250], draws[9749]]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--profile', type=pathlib.Path, required=True)
    parser.add_argument('--baseline', type=pathlib.Path, required=True)
    parser.add_argument('--variant', action='append', required=True, help='NAME=BINARY')
    parser.add_argument('--work-dir', type=pathlib.Path, required=True)
    parser.add_argument('--output', type=pathlib.Path, required=True)
    parser.add_argument('--rounds', type=int, default=11)
    args = parser.parse_args()
    if args.rounds < 5:
        parser.error('use at least five pairs')
    profile = json.loads(args.profile.read_text())
    binaries = {'baseline': args.baseline.resolve()}
    for spec in args.variant:
        name, binary = spec.split('=', 1)
        if name in binaries:
            parser.error(f'duplicate variant {name}')
        binaries[name] = pathlib.Path(binary).resolve()
    work = args.work_dir.resolve()
    work.mkdir(parents=True, exist_ok=True)
    report = {'git_head': subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
              'host': platform.platform(), 'profile_sha256': sha(args.profile),
              'script_sha256': sha(pathlib.Path(__file__)), 'seed': 20261015,
              'rounds': args.rounds,
              'method': 'Serial warm-cache library calls; raw top-level input resident, includes transitive header IO and internal destruction; excludes startup, raw input loading, SDK discovery and returned text destruction. Same corpus/options/SDK for every binary. Each process has one untimed whole-app warmup. Applications shuffled; variant order rotates across paired rounds. Comparisons use same-round variant/baseline ratios. 95% bootstrap intervals resample pairs within the fixed application; no claim about other applications or hardware.',
              'binaries': {name: {'path': str(p), 'sha256': sha(p)} for name, p in binaries.items()},
              'sources': {str(p): sha(p) for p in [pathlib.Path('crates/keel-c/src/preprocess.rs'), pathlib.Path('crates/keel-c/examples/profile_preprocess.rs')]},
              'validation': {}, 'samples': [], 'commands': [], 'summaries': {}}
    apps = list(profile['preprocessing'])
    for app in apps:
        row = profile['preprocessing'][app]
        for source in row['sources']:
            assert sha(pathlib.Path(source['path'])) == source['sha256'], source['path']
        hashes = {}
        for name, binary in binaries.items():
            output = work / name / app
            argv = command(binary, 'prepare', row['prepare_command'], output)
            report['commands'].append(argv)
            run(argv)
            hashes[name] = {p.name: sha(p) for p in sorted(output.glob('*.i'))}
            assert len(hashes[name]) == len(profile['inputs'][app])
            assert hashes[name] == hashes['baseline'], f'output mismatch: {name}/{app}'
        report['validation'][app] = {'byte_identical': True, 'files': len(hashes['baseline']), 'output_sha256': hashes['baseline'], 'IR_verified_by_each_binary': True}
        print(f'verified {app}: {len(hashes["baseline"])} byte-identical files in {len(binaries)} variants', flush=True)
    args.output.parent.mkdir(parents=True, exist_ok=True)

    def save():
        args.output.write_text(json.dumps(report, indent=2) + '\n')

    rng = random.Random(report['seed'])
    names = list(binaries)
    for round_index in range(args.rounds):
        order = apps.copy()
        rng.shuffle(order)
        for app in order:
            shift = round_index % len(names)
            for name in names[shift:] + names[:shift]:
                argv = command(binaries[name], 'time', profile['preprocessing'][app]['prepare_command'], work / name / app)
                report['commands'].append(argv)
                rows = [json.loads(line) for line in run(argv).splitlines()]
                report['samples'].append({'app': app, 'variant': name, 'round': round_index,
                                          'total_ms': sum(r['preprocess_ms'] for r in rows),
                                          'output_drop_ms': sum(r['output_drop_ms'] for r in rows), 'files': rows})
        print(f'paired round {round_index + 1}/{args.rounds}', flush=True)
        save()
    for app in apps:
        selected = [r for r in report['samples'] if r['app'] == app]
        times = {name: [r['total_ms'] for r in selected if r['variant'] == name] for name in names}
        result = {'median_ms': {k: statistics.median(v) for k, v in times.items()}, 'comparisons': {}}
        for name in names[1:]:
            ratios = [a / b for a, b in zip(times[name], times['baseline'])]
            result['comparisons'][name] = {'paired_ratios': ratios, 'median_ratio': statistics.median(ratios), 'ratio_ci95': interval(ratios, rng), 'speedup': 1 / statistics.median(ratios)}
        if 'compact-scalar' in times and 'simd' in times:
            ratios = [a / b for a, b in zip(times['simd'], times['compact-scalar'])]
            result['simd_vs_scalar'] = {'paired_ratios': ratios, 'median_ratio': statistics.median(ratios), 'ratio_ci95': interval(ratios, rng)}
        report['summaries'][app] = result
    report['aggregate'] = {}
    for name in names[1:]:
        ratios = [report['summaries'][app]['comparisons'][name]['paired_ratios'] for app in apps]
        center = math.exp(statistics.mean(math.log(statistics.median(r)) for r in ratios))
        draws = sorted(math.exp(statistics.mean(math.log(statistics.median(rng.choices(r, k=len(r)))) for r in ratios)) for _ in range(10000))
        report['aggregate'][name] = {'equal_application_geomean_ratio': center, 'ratio_ci95': [draws[250], draws[9749]], 'speedup': 1 / center}
    for name, binary in binaries.items():
        assert sha(binary) == report['binaries'][name]['sha256'], f'binary changed during timing: {name}'
    save()
    print(json.dumps({'summaries': report['summaries'], 'aggregate': report['aggregate']}, indent=2))


if __name__ == '__main__':
    main()
