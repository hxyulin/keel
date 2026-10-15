"""Profile merged AArch64 instruction families using checked-in correctness cases.

Build `keel` and the keel-asm `profile` example in release mode first. Workloads
repeat named test cases to cover operand parsing; they are assembler stress
inputs, not executable programs or CPU execution benchmarks. Only the three
matrix kernels are real function fixtures. Source generation, timing and macOS
stack sampling run serially; the report retains inputs, hashes and samples.
"""
import argparse
import hashlib
import json
import math
import pathlib
import platform
import re
import statistics
import subprocess
import time

ROOT = pathlib.Path(__file__).resolve().parents[1]


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(command):
    result = subprocess.run(list(map(str, command)), stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if result.returncode:
        raise RuntimeError(f"{list(map(str, command))}: {result.stderr.decode(errors='replace')}")
    return result.stdout


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--work-dir', type=pathlib.Path, required=True)
    parser.add_argument('--output', type=pathlib.Path, required=True)
    parser.add_argument('--rounds', type=int, default=11)
    parser.add_argument('--sample-seconds', type=int, default=3)
    parser.add_argument('--mib', type=int, default=4)
    args = parser.parse_args()
    if args.rounds < 5 or args.mib < 1 or not 1 <= args.sample_seconds <= 30:
        parser.error('use >=5 rounds, >=1 MiB and 1..30 sampling seconds')
    work = args.work_dir.resolve()
    work.mkdir(parents=True, exist_ok=True)
    cli = ROOT / 'target/release/keel'
    profiler = ROOT / 'target/release/examples/profile'
    tests = ROOT / 'crates/keel-asm/tests'
    report = {'git_head': run(['git', 'rev-parse', 'HEAD']).decode().strip(),
              'host': platform.platform(), 'rounds': args.rounds,
              'script_sha256': sha(pathlib.Path(__file__)),
              'binaries': {p.name: {'path': str(p), 'sha256': sha(p)} for p in [cli, profiler]},
              'method': 'Warm caches; serial processes. Library timings exclude source reading/validation and result destruction. CLI includes startup, mmap, UTF-8 validation, parsing/encoding, object writing and destruction. CPU samples are wall-clock stack samples, not cycle counters. Feature inputs are assembled only; no generated stress code is executed. No claim that case coverage is exhaustive syntax coverage.',
              'workloads': {}}
    fixtures = {}
    for name, mode in [('neon', 'baseline'), ('numeric', 'apple-m5'),
                       ('sme', 'apple-m5'), ('streaming', 'sve')]:
        case_file = tests / f'{name}-cases.txt'
        cases = [line.replace('\\t', '\t').split('\t', 1)
                 for line in case_file.read_text().splitlines()
                 if line and not line.startswith('#')]
        seed = '\n'.join(source for _, source in cases) + '\n'
        if name == 'streaming':
            seed += 'ptrue p0.s\npfalse p1.b\nadd z0.s, z1.s, z2.s\nsub z0.d, z1.d, z2.d\nfadd z0.s, z1.s, z2.s\nfsub z0.d, z1.d, z2.d\nfmul z0.s, z1.s, z2.s\nrdvl x0, #1\n'
        repeats = math.ceil(args.mib * 1024 * 1024 / len(seed.encode()))
        source = work / f'{name}.s'
        source.write_text('.text\n' + seed * repeats)
        fixtures[name] = (source, mode, {'case_file': str(case_file.relative_to(ROOT)),
                         'case_sha256': sha(case_file), 'named_case_encodings': [name for name, _ in cases],
                         'repetitions': repeats})
    kernel_files = sorted((tests / 'fixtures/sme-kernels').glob('*.s'))
    kernel = work / 'matrix-kernels.s'
    kernel.write_text('\n'.join(p.read_text() for p in kernel_files))
    fixtures['matrix_kernels'] = (kernel, 'apple-m5', {'fixtures': [{'path': str(p.relative_to(ROOT)), 'sha256': sha(p)} for p in kernel_files]})
    fixtures['lua_lvm'] = (ROOT / 'target/suite/lua/build/keel/lvm.s', 'baseline', {})
    fixtures['compiler_debug'] = (ROOT / 'target/assembler-bench/compiler-debug.s', 'baseline', {})

    def save():
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report, indent=2) + '\n')

    for name, (source, mode, metadata) in fixtures.items():
        stage_command = [profiler, source, args.rounds, mode]
        stage = json.loads(run(stage_command))
        row = {'source': str(source), 'source_bytes': source.stat().st_size,
               'source_sha256': sha(source), 'feature_selection': mode,
               'stage_command': list(map(str, stage_command)), 'stage': stage,
               'metadata': metadata, 'stage_median_ms': {key: statistics.median(stage[key]) for key in ['assemble_ms', 'macho_ms']}}
        if mode != 'sve':
            command = [cli, 'asm', '--cpu', mode, source, '-o', work / f'{name}.o']
            for _ in range(2):
                run(command)
            samples = []
            for _ in range(args.rounds):
                start = time.perf_counter_ns()
                run(command)
                samples.append((time.perf_counter_ns() - start) / 1e6)
            row.update(cli_command=list(map(str, command)), cli_ms=samples, cli_median_ms=statistics.median(samples))
        else:
            row['cli_note'] = 'CLI exposes baseline/apple-m5; explicit SVE selection is measured through the library API.'
        report['workloads'][name] = row
        save()
        print(name, row['stage_median_ms'], row.get('cli_median_ms'), flush=True)

    # Collect stacks after timings so the sampler cannot distort measured time.
    for name, (source, mode, _) in fixtures.items():
        raw_path = work / f'{name}-sample.txt'
        command = [profiler, source, 1000000, mode]
        with (work / f'{name}-stdout.log').open('w') as stdout, (work / f'{name}-stderr.log').open('w') as stderr:
            process = subprocess.Popen(list(map(str, command)), stdout=stdout, stderr=stderr)
            try:
                time.sleep(.15)
                sample_command = ['sample', str(process.pid), str(args.sample_seconds), '1', '-file', str(raw_path)]
                run(sample_command)
            finally:
                if process.poll() is None:
                    process.terminate()
                process.wait()
        raw = raw_path.read_text()
        total = sum(map(int, re.findall(r'^    (\d+) Thread_', raw, re.M)))
        tail = raw.split('Sort by top of stack, same collapsed (when >= 5):', 1)[1].split('Binary Images:', 1)[0]
        top = []
        for line in tail.splitlines():
            match = re.match(r'\s+(.+?)\s+\(in (.+?)\)\s+(\d+)\s*$', line)
            if match:
                count = int(match[3])
                top.append({'symbol': match[1], 'image': match[2], 'samples': count,
                            'percent_of_thread_samples': count / total * 100})
        report['workloads'][name]['profile'] = {'command': list(map(str, command)),
            'sample_command': sample_command, 'raw_path': str(raw_path),
            'raw_sha256': sha(raw_path), 'thread_root_samples': total, 'top_of_stack': top}
        save()
        print(name, 'profile complete', flush=True)


if __name__ == '__main__':
    main()
