"""Paired comparison of independently built assembler experiments.

Supply --candidate NAME CLI PROFILE for baseline and each experiment. All
builds/tests must finish before running this script. No Clang or compilation
is timed. Each input must produce a byte-identical Mach-O across candidates.
"""
import argparse
import hashlib
import json
import pathlib
import platform
import random
import statistics
import subprocess
import time


def run(command):
    result = subprocess.run(list(map(str, command)), capture_output=True)
    if result.returncode:
        raise RuntimeError(f'{command!r}: {result.stderr.decode(errors="replace")}')
    return result.stdout


def summary(samples):
    rng = random.Random(42)
    result = {}
    for name, values in samples.items():
        ratios = [a / b for a, b in zip(values, samples['baseline'])]
        boots = sorted(statistics.median(rng.choices(ratios, k=len(ratios))) for _ in range(2000))
        result[name] = {'median_ms': statistics.median(values),
                        'over_baseline': statistics.median(ratios),
                        'ratio_95_interval': [boots[49], boots[1949]]}
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--candidate', nargs=3, action='append', required=True,
                        metavar=('NAME', 'CLI', 'PROFILE'))
    parser.add_argument('--input-dir', type=pathlib.Path, required=True)
    parser.add_argument('--extra-input', type=pathlib.Path, action='append', default=[])
    parser.add_argument('--work-dir', type=pathlib.Path, required=True)
    parser.add_argument('--output', type=pathlib.Path, required=True)
    parser.add_argument('--rounds', type=int, default=15)
    parser.add_argument('--inner', type=int, default=5)
    args = parser.parse_args()
    candidates = {name: (pathlib.Path(cli).resolve(), pathlib.Path(profile).resolve())
                  for name, cli, profile in args.candidate}
    if 'baseline' not in candidates or len(candidates) != len(args.candidate):
        parser.error('use unique candidate names including baseline')
    if args.rounds < 5 or args.inner < 1:
        parser.error('use at least 5 rounds and 1 inner iteration')
    work = args.work_dir.resolve()
    work.mkdir(parents=True, exist_ok=True)
    lua = sorted(args.input_dir.resolve().glob('*.s'))
    if not lua:
        parser.error('no .s inputs')
    # Deliberate stress inputs, reported separately from real compiler output.
    long_strings = work / 'long-strings.s'
    long_strings.write_text('.data\n' + ('.asciz "' + 'a' * 500 + '; // quoted text"\n') * 2048)
    long_lines = work / 'long-instruction-lines.s'
    long_lines.write_text('.text\n' + (' ' * 128 + 'add x0, x1, #17\n') * 10000)
    inputs = [(p, 'lua') for p in lua]
    inputs += [(p.resolve(), 'compiler-or-mixed') for p in args.extra_input]
    inputs += [(p, 'scanning-stress') for p in (long_strings, long_lines)]
    report = {
        'host': platform.platform(), 'rounds': args.rounds, 'inner': args.inner,
        'method': 'Randomized candidate order for each input in each paired round. Warm caches. CLI includes process startup and I/O. In-process stage samples exclude source reading and result destruction. Group totals sum per-file samples within each round; not parallel build wall time.',
        'script_sha256': hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest(),
        'process_load_at_start': run(['ps', '-axo', 'pid,pcpu,comm']).decode(),
        'candidates': {name: {'cli': str(cli), 'profiler': str(profile),
                            'cli_sha256': hashlib.sha256(cli.read_bytes()).hexdigest(),
                            'profiler_sha256': hashlib.sha256(profile.read_bytes()).hexdigest()}
                       for name, (cli, profile) in candidates.items()},
        'rows': [],
    }
    rng = random.Random(42)
    for index, (path, group) in enumerate(inputs):
        outputs = {name: work / f'{index}-{name}.o' for name in candidates}
        commands = {name: [cli, 'asm', path, '-o', outputs[name]]
                    for name, (cli, _) in candidates.items()}
        for _ in range(2):
            for name in candidates:
                run(commands[name])
        reference = outputs['baseline'].read_bytes()
        assert all(p.read_bytes() == reference for p in outputs.values()), path
        samples = {stage: {name: [] for name in candidates}
                   for stage in ('cli', 'assemble', 'macho')}
        for _ in range(args.rounds):
            order = list(candidates)
            rng.shuffle(order)
            for name in order:
                start = time.perf_counter_ns()
                run(commands[name])
                samples['cli'][name].append((time.perf_counter_ns() - start) / 1e6)
            rng.shuffle(order)
            for name in order:
                stage = json.loads(run([candidates[name][1], path, args.inner]))
                for key, field in [('assemble', 'assemble_ms'), ('macho', 'macho_ms')]:
                    samples[key][name].append(statistics.median(stage[field]))
        row = {'name': path.stem, 'source': str(path), 'group': group,
               'source_bytes': path.stat().st_size,
               'source_sha256': hashlib.sha256(path.read_bytes()).hexdigest(),
               'objects_identical': True, 'samples_ms': samples,
               'summary': {key: summary(values) for key, values in samples.items()}}
        report['rows'].append(row)
        report['groups'] = {}
        for label in sorted({row['group'] for row in report['rows']}):
            rows = [row for row in report['rows'] if row['group'] == label]
            totals = {key: {name: [sum(row['samples_ms'][key][name][i] for row in rows)
                                  for i in range(args.rounds)] for name in candidates}
                      for key in samples}
            report['groups'][label] = {
                'files': len(rows), 'source_bytes': sum(row['source_bytes'] for row in rows),
                'samples_ms': totals,
                'summary': {key: summary(values) for key, values in totals.items()}}
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report, indent=2) + '\n')
        print(path.stem, {name: round(row['summary']['assemble'][name]['over_baseline'], 3)
                          for name in candidates}, flush=True)


if __name__ == '__main__':
    main()
