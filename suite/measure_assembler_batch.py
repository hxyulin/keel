#!/usr/bin/env python3
"""Measure serial single-file invocations versus one Keel batch on macOS arm64.

Build release Keel first. Includes CLI startup, input and object output, excludes
C compilation. Keeps raw paired samples and checks every output before timing.
"""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import random
import shutil
import statistics
import subprocess
import time

from measure_assembler import run, sections


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def interval(values, rng):
    boots = sorted(statistics.median(rng.choices(values, k=len(values))) for _ in range(10000))
    return [boots[249], boots[9749]]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--compiler', type=Path, default=Path('target/release/keel'))
    parser.add_argument('--baseline-compiler', type=Path, help='optional prior batch-capable Keel binary')
    parser.add_argument('--input-dir', type=Path, required=True)
    parser.add_argument('--work-dir', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--pairs', type=int, default=11)
    parser.add_argument('--warmups', type=int, default=3)
    parser.add_argument('--tiny-files', type=int, default=64)
    args = parser.parse_args()
    if args.pairs < 3 or args.warmups < 1 or args.tiny_files < 1:
        parser.error('require >=3 pairs, >=1 warmup and >=1 tiny file')
    inputs = sorted(args.input_dir.resolve().glob('*.s'))
    if not inputs:
        parser.error('--input-dir contains no .s files')
    if len({p.stem for p in inputs}) != len(inputs):
        parser.error('inputs must have distinct output stems')
    compiler = args.compiler.resolve()
    clang = Path(shutil.which('clang')).resolve()
    work = args.work_dir.resolve()
    tiny = work / 'tiny-inputs'
    tiny.mkdir(parents=True, exist_ok=True)
    tiny_inputs = []
    for i in range(args.tiny_files):
        path = tiny / f'function-{i}.s'
        path.write_text(f'.text\n.globl _function_{i}\n_function_{i}:\nadd x0,x0,#{i % 4096}\nret\n')
        tiny_inputs.append(path)
    report = {
        'commit': run(['git', 'rev-parse', 'HEAD']).decode().strip(),
        'host': platform.platform(),
        'cpu': run(['sysctl', '-n', 'machdep.cpu.brand_string']).decode().strip(),
        'clang': run([clang, '--version']).decode(),
        'keel_sha256': sha(compiler), 'script_sha256': sha(Path(__file__)),
        'pairs': args.pairs, 'warmups': args.warmups, 'cpu_selection': 'apple-m5',
        'method': 'Serial CLI wall time, randomized paired warm-cache rounds. Each mode assembles all files; batch uses one process, other modes one process per file. Includes input and output; excludes compilation, linking and verification. All batches match per-file Keel objects exactly, and instruction bytes match Clang. Does not isolate process spawn from other per-process initialization or compare parallel builds.',
        'rows': [],
    }
    if args.baseline_compiler:
        report['baseline_keel_sha256'] = sha(args.baseline_compiler.resolve())
    rng = random.Random(20261015)
    for name, sources in [('input-directory', inputs), ('tiny-functions', tiny_inputs)]:
        dirs = {mode: work / name / mode for mode in ['keel-per-file', 'keel-batch', 'clang-cc1as']}
        if args.baseline_compiler:
            dirs['keel-batch-before'] = work / name / 'keel-batch-before'
        for directory in dirs.values():
            directory.mkdir(parents=True, exist_ok=True)
        commands = {
            'keel-per-file': [[str(compiler), 'asm', '--cpu', 'apple-m5', str(path), '-o', str(dirs['keel-per-file'] / (path.stem + '.o'))] for path in sources],
            'keel-batch': [[str(compiler), 'asm', '--cpu', 'apple-m5', *map(str, sources), '--out-dir', str(dirs['keel-batch'])]],
            'clang-cc1as': [[str(clang), '-cc1as', '-triple', 'arm64-apple-macosx11.0.0', '-target-cpu', 'apple-m5', '-filetype', 'obj', str(path), '-o', str(dirs['clang-cc1as'] / (path.stem + '.o'))] for path in sources],
        }
        if args.baseline_compiler:
            commands['keel-batch-before'] = [[str(args.baseline_compiler.resolve()), 'asm', '--cpu', 'apple-m5', *map(str, sources), '--out-dir', str(dirs['keel-batch-before'])]]

        def invoke(mode):
            start = time.perf_counter_ns()
            for command in commands[mode]:
                run(command)
            return (time.perf_counter_ns() - start) / 1e6

        for mode in commands:
            invoke(mode)
        for path in sources:
            files = {mode: directory / (path.stem + '.o') for mode, directory in dirs.items()}
            assert files['keel-per-file'].read_bytes() == files['keel-batch'].read_bytes(), path
            if args.baseline_compiler:
                assert files['keel-batch-before'].read_bytes() == files['keel-batch'].read_bytes(), path
            assert sections(files['keel-batch'])['__TEXT,__text'] == sections(files['clang-cc1as'])['__TEXT,__text'], path
        samples = {mode: [] for mode in commands}
        for i in range(args.warmups + args.pairs):
            order = list(commands)
            rng.shuffle(order)
            for mode in order:
                elapsed = invoke(mode)
                if i >= args.warmups:
                    samples[mode].append(elapsed)
        batch_ratios = [a / b for a, b in zip(samples['keel-per-file'], samples['keel-batch'])]
        clang_ratios = [a / b for a, b in zip(samples['clang-cc1as'], samples['keel-batch'])]
        row = {
            'name': name, 'files': len(sources), 'source_bytes': sum(p.stat().st_size for p in sources),
            'sources': [{'path': str(p), 'sha256': sha(p)} for p in sources],
            'commands': commands, 'samples_ms': samples,
            'median_ms': {k: statistics.median(v) for k, v in samples.items()},
            'per_file_over_batch': statistics.median(batch_ratios),
            'per_file_over_batch_bootstrap_95': interval(batch_ratios, rng),
            'clang_over_batch': statistics.median(clang_ratios),
            'clang_over_batch_bootstrap_95': interval(clang_ratios, rng),
            'batch_objects_identical': True, 'text_matches_clang': True,
        }
        if args.baseline_compiler:
            ratios = [a / b for a, b in zip(samples['keel-batch'], samples['keel-batch-before'])]
            row['batch_after_over_before'] = statistics.median(ratios)
            row['batch_after_over_before_bootstrap_95'] = interval(ratios, rng)
        report['rows'].append(row)
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report, indent=2) + '\n')
        print(name, row['median_ms'], 'per-file / batch:', row['per_file_over_batch'], flush=True)


if __name__ == '__main__':
    main()
