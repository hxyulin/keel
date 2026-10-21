#!/usr/bin/env python3
"""Standalone extended corpus. Entry point owns the shared measurement lock."""
import argparse
import hashlib
import html
import json
import os
from pathlib import Path
import platform
import random
import shutil
import statistics
import subprocess
import sys
import time

from oracle import CASES, LENGTHS, MASK, inputs, reference

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def compiler_configurations(include_gcc=False):
    result = {f'keel-{tier}': ('keel', ['cc', *([] if tier == 'none' else ['-O']),
                                       '--opt-tier', tier, '--emit-object'])
              for tier in ('none', 'fast', 'full')}
    result['clang-O2'] = ('clang', ['-std=c11', '-O2', '-c'])
    if include_gcc:
        result['gcc-O2'] = ('gcc', ['-std=c11', '-O2', '-c'])
    return result


def canonical_runtime(binary, output):
    # Link then rename, never overwrite a running executable's inode. All
    # variants have the same argv[0], while original artifacts remain intact.
    canonical = output / 'run'
    # POSIX rename of two names for the same inode is a no-op, leaving
    # run.next behind. Consecutive jobs for one variant need no replacement.
    if canonical.exists() and canonical.samefile(binary):
        return canonical
    pending = output / 'run.next'
    os.link(binary, pending)
    os.replace(pending, canonical)
    return canonical


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--keel', required=True, type=Path)
    ap.add_argument('--clang', required=True, type=Path)
    ap.add_argument('--gcc', type=Path, help='optional actual GCC, never auto-selected')
    ap.add_argument('--output', required=True, type=Path, help='new evidence directory')
    ap.add_argument('--label', required=True, help='host/architecture/run label')
    ap.add_argument('--cpu', type=int, help='Linux affinity for this run and children only')
    ap.add_argument('--seed', type=int, default=20261021)
    ap.add_argument('--rounds', type=int, default=5)
    ap.add_argument('--compile-rounds', type=int, default=3)
    ap.add_argument('--target-elements', type=int, default=2000000, help='minimum elements per timed sample')
    ap.add_argument('--smoke', action='store_true', help='one round, fewer lengths and repetitions')
    ap.add_argument('--worker', action='store_true', help=argparse.SUPPRESS)
    args = ap.parse_args()
    args.output = args.output.resolve()
    if not 0 < args.seed <= MASK or min(args.rounds, args.compile_rounds, args.target_elements) < 1:
        ap.error('positive counts and nonzero uint64 seed required')
    if not args.worker:
        args.output.mkdir(parents=True, exist_ok=False)
        # Entire worker is serialized, including compilation and correctness.
        # Do not put the public entry point under another fast_opt_locked.py.
        command = [sys.executable, str(ROOT / 'suite/fast_opt_locked.py'), str(args.output / 'lock.jsonl'),
                   sys.executable, str(Path(__file__).resolve()), *sys.argv[1:], '--worker']
        environment = dict(os.environ, KEEL_RUNTIME_LOCK_WORKER=str(args.output))
        return subprocess.call(command, env=environment)
    if os.environ.pop('KEEL_RUNTIME_LOCK_WORKER', None) != str(args.output):
        ap.error('use the public entry point; worker must run under fast_opt_locked.py')
    if not args.output.is_dir():
        ap.error('internal worker requires the entry point-created output directory')
    if args.cpu is not None:
        if not hasattr(os, 'sched_setaffinity'):
            ap.error('--cpu requires Linux sched_setaffinity')
        os.sched_setaffinity(0, {args.cpu})
    if args.smoke:
        args.rounds = args.compile_rounds = 1
        args.target_elements = 32768
    lengths = LENGTHS[:-1] if args.smoke else LENGTHS
    compilers = {'keel': args.keel.resolve(), 'clang': args.clang.resolve()}
    if args.gcc:
        compilers['gcc'] = args.gcc.resolve()
    for path in compilers.values():
        if not path.is_file():
            ap.error(f'missing compiler: {path}')
    configurations = compiler_configurations(args.gcc is not None)
    report = {'schema': 1, 'corpus': 'runtime-kernels-v1', 'label': args.label,
              'host': {'uname': list(platform.uname()), 'machine': platform.machine(),
                       'affinity': sorted(os.sched_getaffinity(0)) if hasattr(os, 'sched_getaffinity') else None,
                       'load_before': os.getloadavg(), 'settings': {}},
              'seed': args.seed, 'lengths': lengths, 'cases': CASES,
              'compilers': {}, 'configurations': configurations, 'sources': {}, 'compile': [], 'correctness': [], 'runtime': [],
              'semantics': {'compile': 'warm process-to-object wall ns; includes startup, source read, frontend, optimization and object emission; excludes driver/link',
                            'runtime': 'CLOCK_MONOTONIC ns; loop, external call, volatile sink and unsigned checksum included; input allocation/generation and initial oracle check excluded',
                            'warmups': 'one fresh object per config before measured compile rounds; one untimed native call per timed process',
                            'reps': 'max(3, target_elements/max(length,1)), capped at 1000000; fixed across configs',
                            'original_21_kernel_corpus': 'excluded; do not merge scores',
                            'runtime_argv0': str(args.output / 'run')}}
    for path in (HERE / 'kernels.c', HERE / 'driver.c', HERE / 'oracle.py', Path(__file__).resolve()):
        report['sources'][path.name] = sha(path)
    settings = report['host']['settings']
    for name in ('scaling_governor', 'scaling_cur_freq'):
        path = Path(f'/sys/devices/system/cpu/cpu{args.cpu or 0}/cpufreq/{name}')
        if path.exists():
            settings[str(path)] = path.read_text().strip()
    for path in (Path('/sys/devices/system/cpu/intel_pstate/no_turbo'), Path('/sys/devices/system/cpu/cpufreq/boost')):
        if path.exists():
            settings[str(path)] = path.read_text().strip()
    rng = random.Random(args.seed)
    commands = (args.output / 'commands.jsonl').open('x')

    def save():
        report['host']['load_after'] = os.getloadavg()
        (args.output / 'correctness.json').write_text(json.dumps(report['correctness'], indent=2) + '\n')
        (args.output / 'results.json').write_text(json.dumps(report, indent=2) + '\n')
        commands.flush()

    def run(argv, check=True):
        argv = list(map(str, argv))
        start = time.perf_counter_ns()
        p = subprocess.run(argv, capture_output=True, text=True, timeout=600)
        ns = time.perf_counter_ns() - start
        row = dict(argv=argv, wall_ns=ns, returncode=p.returncode, stdout=p.stdout, stderr=p.stderr)
        commands.write(json.dumps(row) + '\n')
        commands.flush()
        if p.returncode and check:
            raise RuntimeError(row)
        return p.stdout, ns

    try:
        for name, path in compilers.items():
            version, _ = run([path, '--version'], check=name != 'keel')
            if name == 'keel' and not version.strip():
                version = 'CLI has no --version; executable SHA-256 is authoritative'
            report['compilers'][name] = dict(path=str(path), sha256=sha(path), version=version)
        report['checkout_commit'] = run(['git', '-C', ROOT, 'rev-parse', 'HEAD'], check=False)[0].strip() or None
        if platform.system() == 'Darwin':
            settings['cpu_brand'] = run(['sysctl', '-n', 'machdep.cpu.brand_string'])[0].strip()
        elif shutil.which('lscpu'):
            settings['lscpu'] = run(['lscpu'])[0]
        driver = args.output / 'driver.o'
        run([compilers['clang'], '-std=c11', '-O2', '-c', HERE / 'driver.c', '-o', driver])
        binaries = {}
        for round_id in range(-1, args.compile_rounds):
            order = list(configurations)
            rng.shuffle(order)
            for config in order:
                family, flags = configurations[config]
                directory = args.output / f'{config}-round-{round_id + 1}'
                directory.mkdir()
                source, obj = directory / 'kernels.c', directory / 'kernels.o'
                shutil.copyfile(HERE / 'kernels.c', source)
                _, ns = run([compilers[family], *flags, source, '-o', obj])
                report['compile'].append(dict(config=config, round=round_id, warmup=round_id == -1,
                                              wall_ns=ns, object=str(obj), sha256=sha(obj), bytes=obj.stat().st_size))
                if round_id == args.compile_rounds - 1:
                    binary = directory / 'bench'
                    run([compilers['clang'], driver, obj, '-o', binary])
                    binaries[config] = binary
                    asm_flags = [x for x in flags if x not in ('--emit-object', '-c')]
                    if family != 'keel':
                        asm_flags.append('-S')
                    run([compilers[family], *asm_flags, source, '-o', directory / 'kernels.s'])
                    report['compile'][-1]['assembly'] = str(directory / 'kernels.s')
                    report['compile'][-1]['executable_sha256'] = sha(binary)
        # Every configuration/case/length must pass before ANY runtime sampling.
        expected = {}
        for n in lengths:
            data = inputs(n, args.seed)
            expected[n] = reference(*data)
            # Canonical little-endian input digest, independent of host endianness.
            digest = hashlib.sha256()
            for array, width in zip(data, (4, 8, 8)):
                for value in array:
                    digest.update((value % (1 << (width * 8))).to_bytes(width, 'little'))
            for case in CASES:
                for config, binary in binaries.items():
                    stdout, _ = run([binary, 'check', case, n, 1, args.seed, expected[n][case]])
                    row = json.loads(stdout)
                    if row['result'] != expected[n][case]:
                        raise RuntimeError('oracle output mismatch')
                    report['correctness'].append(dict(config=config, input_sha256=digest.hexdigest(), **row))
        (args.output / 'correctness.json').write_text(json.dumps(report['correctness'], indent=2) + '\n')
        for round_id in range(args.rounds):
            jobs = [(config, case, n) for config in binaries for case in CASES for n in lengths]
            rng.shuffle(jobs)
            for config, case, n in jobs:
                reps = min(1000000, max(3, args.target_elements // max(n, 1)))
                binary = canonical_runtime(binaries[config], args.output)
                stdout, _ = run([binary, 'time', case, n, reps, args.seed, expected[n][case]])
                row = json.loads(stdout)
                if row['checksum'] != (expected[n][case] * reps) & MASK:
                    raise RuntimeError('timed checksum mismatch')
                report['runtime'].append(dict(config=config, round=round_id, ns_per_call=row['elapsed_ns'] / reps, **row))
            save()
        report['status'] = 'passed'
    except Exception as error:
        report['status'] = 'failed'
        report['error'] = str(error)
        raise
    finally:
        save()
        commands.close()
    # Standalone portable report; runtime medians remain per case/length.
    rows = []
    for case in CASES:
        for n in lengths:
            cells = [f'<td>{case}</td><td>{n}</td>']
            for config in configurations:
                values = [r['ns_per_call'] for r in report['runtime'] if r['config'] == config and r['case'] == case and r['length'] == n]
                cells.append(f'<td>{statistics.median(values):.2f}</td>')
            rows.append('<tr>' + ''.join(cells) + '</tr>')
    compile_rows = []
    for config in configurations:
        measured = [r for r in report['compile'] if r['config'] == config and not r['warmup']]
        ms = statistics.median(r['wall_ns'] for r in measured) / 1000000
        compile_rows.append(f'<tr><td>{config}</td><td>{ms:.3f}</td><td>{len(measured)}</td></tr>')
    title = html.escape(args.label)
    (args.output / 'report.html').write_text('<!doctype html><meta charset="utf-8"><title>Extended runtime corpus</title>'
        '<style>body{font:15px system-ui;margin:2em}table{border-collapse:collapse}td,th{padding:6px 12px;border:1px solid #ccc;text-align:right}th{background:#eee}</style>'
        f'<h1>Extended runtime corpus — {title}</h1><p>Independent Python oracle: {len(report["correctness"])} checks passed. '
        f'{args.rounds} runtime rounds. Median ns/call; includes call and checksum overhead. '
        'Original 21 kernels excluded. Smoke results are validation, not performance claims.</p>'
        '<h2>Compile to object</h2><p>Process wall time, excluding driver/link and warmup.</p>'
        '<table><tr><th>Configuration</th><th>Median ms</th><th>Measured rounds</th></tr>' + ''.join(compile_rows) + '</table>'
        '<h2>Runtime</h2><table><tr><th>Case</th><th>Length</th>' + ''.join(f'<th>{c}</th>' for c in configurations) + '</tr>' + ''.join(rows) + '</table>')
    print(json.dumps(dict(status=report['status'], evidence=str(args.output), checks=len(report['correctness']), runtime_samples=len(report['runtime']))))
    return 0


if __name__ == '__main__':
    sys.exit(main())
