#!/usr/bin/env python3
"""Paired 44-file compilation measurements: same Keel, native vs external assembly.

Run only on an idle x86-64 Linux host, after correctness checks. Linking is
excluded. Both paths compile identical preprocessed inputs serially.
"""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import statistics
import subprocess
import time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--compiler', type=Path, required=True)
parser.add_argument('--prepared', type=Path, required=True)
parser.add_argument('--work-dir', type=Path, required=True)
parser.add_argument('--pairs', type=int, default=3)
args = parser.parse_args()
compiler = args.compiler.resolve()
prepared = args.prepared.resolve()
work = args.work_dir.resolve()
if work == prepared or work.is_relative_to(prepared):
    parser.error('work directory must be outside prepared inputs')
work.mkdir(parents=True, exist_ok=True)
inputs = [(suite, path) for suite in ['lua', 'quickjs', 'sqlite']
          for path in sorted((prepared / suite / 'pre').glob('*.i'))]
if len(inputs) != 44:
    parser.error(f'expected the prepared 44-file corpus, found {len(inputs)}')
report = {'host': platform.node(), 'platform': platform.platform(),
          'compiler_sha256': hashlib.sha256(compiler.read_bytes()).hexdigest(),
          'clang': subprocess.check_output(['clang', '--version'], text=True).splitlines()[0],
          'inputs': [{'suite': s, 'path': str(p), 'sha256': hashlib.sha256(p.read_bytes()).hexdigest()}
                     for s, p in inputs], 'configurations': {}}


def batch(native, flags):
    commands = []
    for suite, source in inputs:
        out = work / suite
        out.mkdir(exist_ok=True)
        obj = out / source.with_suffix('.o').name
        if native:
            commands.append([compiler, 'cc', *flags, '--emit-object', source, '-o', obj])
        else:
            asm = out / source.with_suffix('.s').name
            commands.extend([[compiler, 'cc', *flags, source, '-o', asm], ['clang', '-c', asm, '-o', obj]])
    start = time.perf_counter()
    for command in commands:
        subprocess.run([str(a) for a in command], cwd=work, check=True,
                       stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
    return time.perf_counter() - start


for name, flags in [('baseline', []), ('opt', ['-O'])]:
    batch(False, flags)
    batch(True, flags)
    samples = []
    for pair in range(args.pairs):
        sample = {}
        for native in ([False, True] if pair % 2 == 0 else [True, False]):
            sample['native_seconds' if native else 'external_seconds'] = batch(native, flags)
        samples.append(sample)
        print(name, pair + 1, sample, flush=True)
    native = statistics.median(s['native_seconds'] for s in samples)
    external = statistics.median(s['external_seconds'] for s in samples)
    report['configurations'][name] = {'flags': flags, 'samples': samples,
                                      'median_native_seconds': native,
                                      'median_external_seconds': external,
                                      'native_over_external': native / external}
    (work / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
