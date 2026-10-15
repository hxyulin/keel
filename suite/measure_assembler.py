"""Compare release Keel with Clang's driver and direct integrated assembler.

Run on macOS arm64. Builds are deliberately outside the timed region. Sources,
objects and raw paired samples are retained under --output/--work-dir. No C
compilation or preprocessing is included in assembly timings. Synthetic inputs
exercise scaling; generated C exercises symbols, relocations and CFI/DWARF.
"""
import argparse
import hashlib
import json
import pathlib
import platform
import random
import shutil
import statistics
import struct
import subprocess
import time

ROOT = pathlib.Path(__file__).resolve().parent.parent


def run(cmd):
    result = subprocess.run(list(map(str, cmd)), capture_output=True)
    if result.returncode:
        raise RuntimeError(f'{cmd!r}: {result.stderr.decode(errors="replace")}')
    return result.stdout


def sections(path):
    """Read file-backed Mach-O sections; object layout/metadata need not match."""
    data = path.read_bytes()
    assert struct.unpack_from('<I', data)[0] == 0xfeedfacf
    offset = 32
    result = {}
    for _ in range(struct.unpack_from('<I', data, 16)[0]):
        cmd, size = struct.unpack_from('<II', data, offset)
        if cmd == 0x19:
            count = struct.unpack_from('<I', data, offset + 64)[0]
            for index in range(count):
                pos = offset + 72 + index * 80
                name = data[pos:pos + 16].split(b'\0')[0].decode()
                segment = data[pos + 16:pos + 32].split(b'\0')[0].decode()
                length, start = struct.unpack_from('<QI', data, pos + 40)
                result[segment + ',' + name] = data[start:start + length]
        offset += size
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=pathlib.Path, required=True)
    parser.add_argument('--work-dir', type=pathlib.Path, default=ROOT / 'target/assembler-bench')
    parser.add_argument('--pairs', type=int, default=15)
    parser.add_argument('--warmups', type=int, default=3)
    parser.add_argument('--functions', type=int, default=1000)
    parser.add_argument('--baseline-compiler', type=pathlib.Path)
    parser.add_argument('--baseline-profiler', type=pathlib.Path)
    parser.add_argument('--compiler', type=pathlib.Path, help='measure an existing candidate binary without rebuilding')
    parser.add_argument('--profiler', type=pathlib.Path, help='stage profiler built from the same candidate')
    parser.add_argument('--input-dir', type=pathlib.Path, help='also measure every .s file in this directory')
    parser.add_argument('--only-inputs', action='store_true', help='skip synthetic/generated-C inputs')
    args = parser.parse_args()
    if args.pairs < 3 or args.warmups < 1 or args.functions < 1:
        parser.error('use >=3 pairs, >=1 warmup and >=1 function')
    if args.only_inputs and args.input_dir is None:
        parser.error('--only-inputs requires --input-dir')
    if args.baseline_profiler and not args.baseline_compiler:
        parser.error('--baseline-profiler requires --baseline-compiler')
    if bool(args.compiler) != bool(args.profiler):
        parser.error('--compiler and --profiler must be supplied together')
    work = args.work_dir.resolve()
    work.mkdir(parents=True, exist_ok=True)
    clang = pathlib.Path(shutil.which('clang')).resolve()
    if args.compiler:
        keel = args.compiler.resolve()
        profiler = args.profiler.resolve()
    else:
        run(['cargo', 'build', '--release', '-p', 'keel', '-p', 'keel-asm', '--example', 'profile', '--bin', 'keel'])
        keel = ROOT / 'target/release/keel'
        profiler = ROOT / 'target/release/examples/profile'
    sources = []

    def source(name, text):
        path = work / (name + '.s')
        path.write_text(text)
        sources.append(path)

    mixed = ('add x0, x1, #17\nsub w3, w4, w5, lsl #2\n'
             'ldr x6, [x7, #24]\nstr d0, [sp, #16]\n'
             'fadd d0, d1, d2\nmovz x8, #1234\n'
             'and x9, x10, x11\nldaddal x0, x1, [x2]\n')
    if not args.only_inputs:
        source('tiny', (ROOT / 'crates/keel-asm/examples/compute_loop.s').read_text())
        for count in (1000, 10000, 100000):
            source(f'mixed-{count}', '.text\n' + mixed * (count // 8))
        c = work / 'functions.i'
        c.write_text('extern long external(long);\n' + ''.join(
            f'long fn{i}(long x, long *p) {{ long y = p[x & 7] + {i}; '
            'if (x > y) y = external(y); return y * 3 + x; }\n'
            for i in range(args.functions)))
        for debug in (False, True):
            path = work / ('compiler-debug.s' if debug else 'compiler.s')
            run([keel, 'cc', *(['-g'] if debug else []), c, '-o', path])
            sources.append(path)
    if args.input_dir:
        inputs = sorted(args.input_dir.resolve().glob('*.s'))
        if not inputs:
            parser.error('--input-dir contains no .s files')
        sources.extend(inputs)

    report = {
        'commit': run(['git', 'rev-parse', 'HEAD']).decode().strip(),
        'host': platform.platform(),
        'cpu': run(['sysctl', '-n', 'machdep.cpu.brand_string']).decode().strip(),
        'clang': run([clang, '--version']).decode(),
        'rustc': run(['rustc', '--version']).decode().strip(),
        'keel_sha256': hashlib.sha256(keel.read_bytes()).hexdigest(),
        'script_sha256': hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest(),
        'pairs': args.pairs, 'warmups': args.warmups,
        'functions': args.functions, 'rows': [],
        'scope': 'Warm filesystem cache, serial subprocess wall time including startup, file input and object output; no preprocessing. In-process stages exclude input I/O and destruction. Coverage is limited to the listed synthetic/generated-C or supplied assembly inputs.',
    }
    if args.baseline_compiler:
        report['baseline_keel_sha256'] = hashlib.sha256(args.baseline_compiler.read_bytes()).hexdigest()
    rng = random.Random(42)
    for path in sources:
        commands = {
            'keel': [str(keel), 'asm', str(path), '-o', str(work / 'keel.o')],
            'clang-driver': [str(clang), '-target', 'arm64-apple-macosx11.0.0', '-mcpu=apple-m1', '-x', 'assembler', '-c', str(path), '-o', str(work / 'clang-driver.o')],
            'clang-cc1as': [str(clang), '-cc1as', '-triple', 'arm64-apple-macosx11.0.0', '-target-cpu', 'apple-m1', '-filetype', 'obj', str(path), '-o', str(work / 'clang-cc1as.o')],
        }
        if args.baseline_compiler:
            commands['keel-before'] = [str(args.baseline_compiler.resolve()), 'asm', str(path), '-o', str(work / 'keel-before.o')]
        for _ in range(args.warmups):
            for command in commands.values():
                run(command)
        # Compare instruction bytes, not whole objects: symbol order and unwind
        # representation legitimately differ. Existing tests cover relocations.
        texts = [sections(work / (name + '.o'))['__TEXT,__text'] for name in commands]
        assert all(text == texts[0] for text in texts), path
        samples = {name: [] for name in commands}
        for _ in range(args.pairs):
            order = list(commands)
            rng.shuffle(order)
            for name in order:
                start = time.perf_counter_ns()
                run(commands[name])
                samples[name].append((time.perf_counter_ns() - start) / 1e6)
        stage = json.loads(run([profiler, path, args.pairs]))
        ratios = [a / b for a, b in zip(samples['keel'], samples['clang-cc1as'])]
        boots = sorted(statistics.median(rng.choices(ratios, k=len(ratios))) for _ in range(2000))
        row = {
            'name': path.stem, 'source_bytes': path.stat().st_size,
            'source_sha256': hashlib.sha256(path.read_bytes()).hexdigest(),
            'commands': commands, 'samples_ms': samples,
            'median_ms': {k: statistics.median(v) for k, v in samples.items()},
            'keel_over_cc1as': statistics.median(ratios),
            'keel_over_cc1as_bootstrap_95': [boots[49], boots[1949]],
            'keel_over_driver': statistics.median([a / b for a, b in zip(samples['keel'], samples['clang-driver'])]),
            'in_process': stage, 'text_matches_clang': True,
        }
        if args.baseline_compiler:
            before_ratios = [a / b for a, b in zip(samples['keel'], samples['keel-before'])]
            before_boots = sorted(statistics.median(rng.choices(before_ratios, k=len(before_ratios))) for _ in range(2000))
            row['keel_after_over_before'] = statistics.median(before_ratios)
            row['keel_after_over_before_bootstrap_95'] = [before_boots[49], before_boots[1949]]
        if args.baseline_profiler:
            row['in_process_before'] = json.loads(run([args.baseline_profiler.resolve(), path, args.pairs]))
        report['rows'].append(row)
        # Sum the serial per-file invocation times within each round. This is
        # assembly batch cost, not parallel build wall time or a linked runtime.
        totals = {name: [sum(item['samples_ms'][name][i] for item in report['rows'])
                         for i in range(args.pairs)] for name in commands}
        report['serial_total'] = {
            'files': len(report['rows']),
            'source_bytes': sum(item['source_bytes'] for item in report['rows']),
            'samples_ms': totals,
            'median_ms': {name: statistics.median(values) for name, values in totals.items()},
        }
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report, indent=2) + '\n')
        print(path.stem, row['median_ms'], 'keel/cc1as:', round(row['keel_over_cc1as'], 3), flush=True)


if __name__ == '__main__':
    main()
