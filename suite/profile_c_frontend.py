"""Profile resident preprocessed C, excluding preprocessing and the backend.

Build first (never concurrently with timing):
  CARGO_PROFILE_RELEASE_DEBUG=1 cargo build --release -p keel-c --features profile --example profile
Prepare suite inputs with the existing suite runner, then pass --input-root
containing lua/, quickjs/, sqlite/ directories of .i files (or their pre/ dirs).
"""
import argparse
import hashlib
import json
import pathlib
import platform
import random
import statistics
import subprocess

ROOT = pathlib.Path(__file__).resolve().parents[1]


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--input-root', type=pathlib.Path, required=True)
    parser.add_argument('--source-root', type=pathlib.Path, help='Optional suite work directory: prepare inputs with the native preprocessor and time it')
    parser.add_argument('--output', type=pathlib.Path, required=True)
    parser.add_argument('--profiler', type=pathlib.Path, default=ROOT / 'target/release/examples/profile')
    parser.add_argument('--rounds', type=int, default=9)
    parser.add_argument('--sample-seconds', type=int, default=6)
    args = parser.parse_args()
    if args.rounds < 3 or not 0 <= args.sample_seconds <= 30:
        parser.error('use >=3 rounds and 0..30 sampling seconds')
    args.output.parent.mkdir(parents=True, exist_ok=True)
    binary = args.profiler.resolve()
    pp_report = {}
    pp_binary = binary.with_name('profile_preprocess')
    if args.source_root:
        sdk = subprocess.check_output(['xcrun', '--sdk', 'macosx', '--show-sdk-path'], text=True).strip()
        source_root = args.source_root.resolve()
        raw = {
            'lua': sorted(p for p in (source_root / 'lua/lua-5.4.7/src').glob('*.c') if p.name != 'luac.c'),
            'quickjs': [source_root / 'quickjs/quickjs-2024-01-13' / name for name in ['qjs.c', 'quickjs.c', 'libregexp.c', 'libunicode.c', 'cutils.c', 'quickjs-libc.c', 'libbf.c']] + [source_root / 'quickjs/gen' / name for name in ['repl.c', 'qjscalc.c']],
            'sqlite': [source_root / 'sqlite/sqlite-amalgamation-3460100' / name for name in ['shell.c', 'sqlite3.c']],
        }
        for app, paths in raw.items():
            output_dir = (args.input_root / app).resolve()
            base = [str(pp_binary), 'prepare', '1', app, sdk, str(output_dir), *map(str, paths)]
            subprocess.run(base, check=True, stdout=subprocess.DEVNULL)
            print(f'prepared native {app}', flush=True)
            command = [str(pp_binary), 'time', str(args.rounds), *base[3:]]
            result = subprocess.run(command, check=True, capture_output=True, text=True)
            rows = [json.loads(line) for line in result.stdout.splitlines()]
            totals = [sum(r['preprocess_ms'] for r in rows if r['round'] == i) for i in range(args.rounds)]
            pp_report[app] = {'prepare_command': base, 'command': command, 'samples': rows, 'median_ms': statistics.median(totals), 'round_totals_ms': totals, 'sources': [{'path': str(p), 'sha256': sha(p)} for p in paths]}
            print(f'timed preprocessing {app}: {statistics.median(totals):.2f} ms', flush=True)
    inputs = {}
    for app in ['lua', 'quickjs', 'sqlite']:
        directory = args.input_root / app
        if (directory / 'pre').is_dir():
            directory /= 'pre'
        inputs[app] = sorted(directory.resolve().glob('*.i'))
        if not inputs[app]:
            parser.error(f'no .i files in {directory}')
    report = {
        'git_head': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
        'host': platform.platform(),
        'cpu': subprocess.check_output(['sysctl', '-n', 'machdep.cpu.brand_string'], text=True).strip(),
        'rustc': subprocess.check_output(['rustc', '-Vv'], text=True),
        'script_sha256': sha(pathlib.Path(__file__)),
        'profiler': {'path': str(binary), 'sha256': sha(binary)},
        'implementation': {str(p.relative_to(ROOT)): sha(p) for p in sorted((ROOT / 'crates/keel-c/src').glob('*.rs'))},
        'preprocessing': pp_report,
        'preprocessor_binary_sha256': sha(pp_binary) if pp_report else None,
        'rounds': args.rounds, 'seed': 20261015,
        'method': 'Serial warm-cache resident-input library timings. Each process validates and warms both paths before measured rounds; exact Debug IR equality and IR verification checked for every input. Baseline calls the unchanged public compile API. Staged measures lexing, parsing+type checking, IR generation, parser/token/AST cleanup. Module destruction is separate. Frontend stage timings exclude preprocessing, file IO, IR verification, optimizer, backend or process startup in timed regions. Modes and applications shuffled in each round; source order rotated. sample captures wall-clock stacks in the baseline compile+module-drop loop, not hardware CPU counters. No before/after optimization claim.',
        'preprocess_method': 'Raw top-level sources resident; transitive header reading, native SDK/header discovery, macro expansion, text emission and internal cleanup included. Warm caches; one untimed warmup; output destruction separate. Preprocessed results compiled and IR verified before measurement. Uses suite manifest defines and sources on macOS; SDK resolution via xcrun excluded. Preprocessing times are independent from resident preprocessed frontend timings.',
        'inputs': {app: [{'path': str(p), 'bytes': p.stat().st_size, 'sha256': sha(p)} for p in paths] for app, paths in inputs.items()},
        'commands': [], 'samples': [], 'summaries': {}, 'stack_samples': {},
    }

    def save():
        args.output.write_text(json.dumps(report, indent=2) + '\n')

    rng = random.Random(report['seed'])
    for round_index in range(args.rounds):
        jobs = [(app, debug, mode) for app in inputs for debug in [False, True] for mode in ['baseline', 'staged']]
        rng.shuffle(jobs)
        for app, debug, mode in jobs:
            paths = inputs[app]
            shift = round_index % len(paths)
            ordered = paths[shift:] + paths[:shift]
            command = [str(binary), mode, '1', *map(str, ordered)] + (['--debug'] if debug else [])
            result = subprocess.run(command, capture_output=True, text=True, check=True)
            report['commands'].append(command)
            for line in result.stdout.splitlines():
                row = json.loads(line)
                row.update(app=app, round=round_index)
                report['samples'].append(row)
        print(f'round {round_index + 1}/{args.rounds}', flush=True)
        save()
    for app in inputs:
        for debug in [False, True]:
            rows = [r for r in report['samples'] if r['app'] == app and r['debug'] == debug]
            summary = {}
            for mode in ['baseline', 'staged']:
                selected = [r for r in rows if r['mode'] == mode]
                keys = ['total_ms', 'module_drop_ms'] + (['lex_ms', 'parse_ms', 'irgen_ms', 'cleanup_ms'] if mode == 'staged' else [])
                totals = {key: [sum(r[key] for r in selected if r['round'] == i) for i in range(args.rounds)] for key in keys}
                summary[mode] = {'round_totals': totals, 'median_ms': {k: statistics.median(v) for k, v in totals.items()}, 'min_ms': min(totals['total_ms']), 'max_ms': max(totals['total_ms'])}
            summary['staged_to_baseline_paired_median'] = statistics.median(a / b for a, b in zip(summary['staged']['round_totals']['total_ms'], summary['baseline']['round_totals']['total_ms']))
            report['summaries'][f'{app}:{"debug" if debug else "normal"}'] = summary
    save()
    if args.sample_seconds and platform.system() == 'Darwin':
        for app, debug in [(a, False) for a in inputs] + [('sqlite', True)] + ([(a, 'preprocess') for a in inputs] if pp_report else []):
            name = app + ('-preprocess' if debug == 'preprocess' else '-debug' if debug else '')
            sample_file = args.output.with_name(args.output.stem + '-' + name + '.sample.txt').resolve()
            command = [str(binary), 'sample', '100000', *map(str, inputs[app])] + (['--debug'] if debug else [])
            if debug == 'preprocess':
                command = [str(pp_binary), 'sample', '100000', *pp_report[app]['prepare_command'][3:]]
            with subprocess.Popen(command, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True) as proc:
                try:
                    # Only attach after validation and warmup finish.
                    while True:
                        line = proc.stderr.readline()
                        if line.startswith('ready:'):
                            break
                        if not line:
                            raise RuntimeError(f'profiler exited during warmup ({proc.poll()})')
                    sample_command = ['sample', str(proc.pid), str(args.sample_seconds), '1', '-fullPaths', '-file', str(sample_file)]
                    result = subprocess.run(sample_command, capture_output=True, text=True)
                    report['stack_samples'][name] = {'command': command, 'sample_command': sample_command, 'returncode': result.returncode, 'stderr': result.stderr, 'path': str(sample_file)}
                    if sample_file.exists():
                        report['stack_samples'][name]['sha256'] = sha(sample_file)
                finally:
                    proc.terminate()
                    proc.wait(timeout=10)
            print(f'sampled {name}', flush=True)
            save()


if __name__ == '__main__':
    main()
