#!/usr/bin/env python3
"""Isolate Clang driver preprocessing, assembler and process startup costs."""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import random
import statistics
import subprocess
import time


def run(cmd):
    p = subprocess.run(list(map(str, cmd)), capture_output=True)
    if p.returncode:
        raise RuntimeError(p.stderr.decode(errors='replace'))
    return p.stdout


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--input-dir', type=Path, required=True, help='keel-S directory from compile-boundary measurement')
    ap.add_argument('--work-dir', type=Path, required=True)
    ap.add_argument('--compiler', type=Path, required=True)
    ap.add_argument('--output', type=Path, required=True)
    args = ap.parse_args()
    work = args.work_dir.resolve()
    work.mkdir(parents=True, exist_ok=True)
    inputs = sorted(args.input_dir.resolve().glob('*/*.s'))
    assert len(inputs) == 44, len(inputs)
    empty = work / 'empty.s'
    empty.write_text('')
    mac = platform.system() == 'Darwin'
    triple = 'arm64-apple-macosx27.0.0' if mac else 'x86_64-unknown-linux-gnu'
    modes = ['driver', 'raw-driver', 'cc1as', 'empty-driver', 'empty-raw-driver'] + (['keel'] if mac else [])
    samples = {m: [] for m in modes}
    def command(m, src):
        obj = work / (m + '.o')
        if m.startswith('empty-'):
            src = empty
        if m == 'keel':
            return [args.compiler.resolve(), 'asm', src, '-o', obj]
        if m == 'cc1as':
            return ['clang', '-cc1as', '-triple', triple] + (['-target-cpu', 'apple-m1'] if mac else []) + ['-filetype', 'obj', src, '-o', obj]
        return ['clang'] + (['-x', 'assembler'] if 'raw-driver' in m else []) + ['-c', src, '-o', obj]
    traces = {}
    for mode in ['driver', 'raw-driver']:
        p = subprocess.run(['clang', '-###', *command(mode, inputs[0])[1:]], capture_output=True)
        assert p.returncode == 0
        traces[mode] = p.stderr.decode()
    if mac:
        from measure_assembler import sections
        # Compare all emitted section bytes before timing the alternate driver.
        for src in inputs:
            expected = None
            for mode in ['driver', 'raw-driver', 'cc1as']:
                run(command(mode, src))
                actual = sections(work / (mode + '.o'))
                if expected is None:
                    expected = actual
                assert actual == expected, (src, mode)
    rng = random.Random(20261015)
    orders = []
    for i in range(6):
        order = modes.copy()
        rng.shuffle(order)
        orders.append(order)
        for mode in order:
            start = time.perf_counter_ns()
            for src in inputs:
                run(command(mode, src))
            if i:
                samples[mode].append((time.perf_counter_ns() - start) / 1e6)
        print('Driver round', i, '/5', flush=True)
    report = {'host': platform.node(), 'clang': run(['clang', '--version']).decode(),
              'method': '44 saved unoptimized assembly inputs; serial seeded randomized mode order; one warmup and five paired batches; empty variants launch the same number of processes; no C compilation, preprocessing of C, or linking.',
              'inputs': [{'path':str(p), 'sha256':hashlib.sha256(p.read_bytes()).hexdigest()} for p in inputs],
              'samples_ms':samples,'median_ms':{m:statistics.median(s) for m,s in samples.items()},
              'commands':{m:list(map(str,command(m,inputs[0]))) for m in modes},'driver_traces':traces,'orders':orders,
              'macho_sections_equal':mac}
    args.output.write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps(report['median_ms'],indent=2))


if __name__ == '__main__':
    main()
