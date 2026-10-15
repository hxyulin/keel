"""Compare input validators and optionally paired full CLI assembly.

Build: cargo build --release -p keel --example validate_input
Pass --corpus LABEL=PATH for directories of .s files or individual files.
Inputs must remain immutable. Validation uses warm mappings, excludes mapping
and parsing, rotates five strategies, and retains every sample. Full CLI timing
includes mapping, validation, assembly and writing; rejects object differences.
Unsupported assembly is reported rather than rewritten or silently excluded.
"""
import argparse
import hashlib
import json
import pathlib
import platform
import random
import statistics
import subprocess
import sys
import time

from measure_assembler_candidates import summary


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--validator', type=pathlib.Path, default=pathlib.Path('target/release/examples/validate_input'))
    parser.add_argument('--corpus', action='append', required=True, help='LABEL=directory-or-file')
    parser.add_argument('--baseline', type=pathlib.Path)
    parser.add_argument('--candidate', type=pathlib.Path)
    parser.add_argument('--rounds', type=int, default=21)
    parser.add_argument('--work-dir', type=pathlib.Path, required=True)
    parser.add_argument('--output', type=pathlib.Path, required=True)
    args = parser.parse_args()
    if args.rounds < 5 or bool(args.baseline) != bool(args.candidate):
        parser.error('use >=5 rounds and supply both CLI binaries or neither')
    args.work_dir.mkdir(parents=True, exist_ok=True)
    report = {'host': platform.platform(), 'rounds': args.rounds,
              'command': sys.argv,
              'rustc': subprocess.check_output(['rustc', '--version']).decode().strip(),
              'method': 'Warm mapped validation only; 3 warmups, rotated strategy order, >=16 MiB validation per sample where possible. CLI: 2 warmups, randomized paired process order, serial corpus totals; 95% bootstrap intervals for paired ratios. No cold-storage claim.',
              'script_sha256': sha(pathlib.Path(__file__)),
              'validator_sha256': sha(args.validator), 'groups': {}}
    if args.baseline:
        report['binaries'] = {k: {'path': str(p.resolve()), 'sha256': sha(p)}
                              for k, p in [('baseline', args.baseline), ('candidate', args.candidate)]}
    rng = random.Random(42)
    for corpus in args.corpus:
        label, value = corpus.split('=', 1)
        path = pathlib.Path(value).resolve()
        files = sorted(path.glob('*.s')) if path.is_dir() else [path]
        if not files:
            parser.error(f'empty corpus: {label}')
        validation = json.loads(subprocess.check_output([
            str(args.validator.resolve()), str(args.rounds), *map(str, files)]))
        group = {'files': [{'path': str(p), 'bytes': p.stat().st_size, 'sha256': sha(p)} for p in files],
                 'validation': validation, 'assembly': []}
        report['groups'][label] = group
        print(label, 'validation medians ms:', {k: statistics.median(v) for k, v in validation['samples_ms'].items()}, flush=True)
        if args.baseline:
            for i, source in enumerate(files):
                outputs = {k: args.work_dir.resolve() / f'{label}-{i}-{k}.o' for k in ['baseline', 'candidate']}
                commands = {k: [str(binary.resolve()), 'asm', str(source), '-o', str(outputs[k])]
                            for k, binary in [('baseline', args.baseline), ('candidate', args.candidate)]}
                row = {'source': str(source), 'commands': commands}
                failures = {}
                for name, command in commands.items():
                    result = subprocess.run(command, capture_output=True)
                    if result.returncode:
                        failures[name] = result.stderr.decode(errors='replace')
                if failures:
                    row['unsupported'] = failures
                else:
                    assert outputs['baseline'].read_bytes() == outputs['candidate'].read_bytes(), source
                    row['objects_identical'] = True
                    samples = {k: [] for k in commands}
                    for round_index in range(args.rounds + 2):
                        order = list(commands)
                        rng.shuffle(order)
                        for name in order:
                            start = time.perf_counter_ns()
                            subprocess.run(commands[name], check=True, capture_output=True)
                            ms = (time.perf_counter_ns() - start) / 1e6
                            if round_index >= 2:
                                samples[name].append(ms)
                    row['samples_ms'] = samples
                group['assembly'].append(row)
            accepted = [r for r in group['assembly'] if 'samples_ms' in r]
            if accepted:
                totals = {name: [sum(r['samples_ms'][name][i] for r in accepted) for i in range(args.rounds)]
                          for name in ['baseline', 'candidate']}
                group['cli_totals'] = {'accepted_files': len(accepted), 'samples_ms': totals, 'summary': summary(totals)}
                print(label, 'CLI:', group['cli_totals']['summary'], flush=True)
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report, indent=2) + '\n')


if __name__ == '__main__':
    main()
