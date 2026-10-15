#!/usr/bin/env python3
"""Paired old/new CLI and in-process timings without repeating a Clang run.

Use after a differential Clang run. This comparison checks both builds' text
bytes, retains binary/source hashes and commands, and never executes fixtures.
"""
import argparse
import hashlib
import json
from pathlib import Path
import random
import statistics
import subprocess
import time
from measure_assembler import sections


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(command):
    return subprocess.check_output(list(map(str, command)))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--before', type=Path, required=True)
    parser.add_argument('--before-profiler', type=Path, required=True)
    parser.add_argument('--after', type=Path, required=True)
    parser.add_argument('--after-profiler', type=Path, required=True)
    parser.add_argument('--input', type=Path, action='append', required=True)
    parser.add_argument('--work-dir', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--pairs', type=int, default=9)
    args = parser.parse_args()
    if args.pairs < 3:
        parser.error('use at least three pairs')
    args.work_dir.mkdir(parents=True, exist_ok=True)
    report = {'commit': run(['git', 'rev-parse', 'HEAD']).decode().strip(),
              'scope': 'Warm serial CLI subprocess wall time and in-process parse/write. apple-m5 feature selection. No execution. Source text matches checked between builds; Clang comparison is recorded separately.',
              'binaries': {name: {'path': str(path), 'sha256': sha(path)} for name, path in [('before', args.before), ('after', args.after), ('before_profiler', args.before_profiler), ('after_profiler', args.after_profiler)]}, 'rows': []}
    rng = random.Random(42)
    for source in args.input:
        commands = {name: [binary, 'asm', '--cpu', 'apple-m5', source, '-o', args.work_dir / f'{name}.o'] for name, binary in [('before', args.before), ('after', args.after)]}
        for command in commands.values():
            run(command)
        assert sections(args.work_dir / 'before.o')['__TEXT,__text'] == sections(args.work_dir / 'after.o')['__TEXT,__text']
        samples = {name: [] for name in commands}
        for _ in range(args.pairs):
            order = list(commands)
            rng.shuffle(order)
            for name in order:
                start = time.perf_counter_ns()
                run(commands[name])
                samples[name].append((time.perf_counter_ns() - start) / 1e6)
        stages = {}
        for name, profiler in [('before', args.before_profiler), ('after', args.after_profiler)]:
            command = [profiler, source, args.pairs, 'apple-m5']
            stages[name] = {'command': command, 'samples': json.loads(run(command))}
        row = {'name': source.stem, 'source': str(source), 'source_bytes': source.stat().st_size, 'source_sha256': sha(source), 'commands': commands,
               'samples_ms': samples, 'in_process': stages, 'text_matches_before': True,
               'cli_after_over_before': statistics.median(a / b for a, b in zip(samples['after'], samples['before'])),
               'parse_after_over_before': statistics.median(stages['after']['samples']['assemble_ms']) / statistics.median(stages['before']['samples']['assemble_ms'])}
        report['rows'].append(row)
        args.output.write_text(json.dumps(report, indent=2, default=str) + '\n')
        print(source.name, row['cli_after_over_before'], row['parse_after_over_before'], flush=True)


if __name__ == '__main__':
    main()
