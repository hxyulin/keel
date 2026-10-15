"""Profile optimized suite applications without enabling frontend debug lowering.

Print optimized IR, add line tables while compiling that IR, and require exact
object .text parity with ordinary `cc -O`. Retain inputs, assembly, binaries,
Clang vectorization remarks, native profiles and correctness outputs.
"""
import argparse
import collections
import concurrent.futures
import hashlib
import json
import math
import pathlib
import platform
import re
import struct
import subprocess
import time

ROOT = pathlib.Path(__file__).resolve().parent.parent
APPS = {"lua": ("lua", ["fib", "sort", "string", "float"]),
        "quickjs": ("qjs", ["fib", "sort", "string"]),
        "sqlite": ("sqlite3", ["queries"])}


def run(command, **kwargs):
    kwargs.setdefault('timeout', 180)
    result = subprocess.run(list(map(str, command)), capture_output=True, **kwargs)
    if result.returncode:
        raise RuntimeError(f"{command}: {result.returncode}\n{result.stderr.decode(errors='replace')}")
    return result


def sha(path):
    return hashlib.sha256(pathlib.Path(path).read_bytes()).hexdigest()


def text_bytes(path):
    data = pathlib.Path(path).read_bytes()
    if data[:4] == b'\x7fELF':
        offset = struct.unpack_from('<Q', data, 40)[0]
        size, count, names_index = struct.unpack_from('<HHH', data, 58)
        names_header = offset + size * names_index
        names_offset, names_size = struct.unpack_from('<QQ', data, names_header + 24)
        names = data[names_offset:names_offset + names_size]
        for index in range(count):
            section = offset + size * index
            name = struct.unpack_from('<I', data, section)[0]
            if names[name:].split(b'\0', 1)[0] == b'.text':
                start, length = struct.unpack_from('<QQ', data, section + 24)
                return data[start:start + length]
    elif data[:4] == b'\xcf\xfa\xed\xfe':
        count = struct.unpack_from('<I', data, 16)[0]
        offset = 32
        for _ in range(count):
            command, size = struct.unpack_from('<II', data, offset)
            if command == 0x19:
                sections = struct.unpack_from('<I', data, offset + 64)[0]
                for index in range(sections):
                    section = offset + 72 + 80 * index
                    if data[section:section + 16].rstrip(b'\0') == b'__text':
                        length, start = struct.unpack_from('<QI', data, section + 40)
                        return data[start:start + length]
            offset += size
    raise ValueError(f'no native .text section: {path}')


def build(args, app, work):
    directory = work / app
    directory.mkdir(parents=True, exist_ok=True)
    sources = sorted((args.prepared / app / 'pre').glob('*.i'))
    if not sources:
        raise ValueError(f'no prepared inputs for {app}')

    def unit(source):
        stem = directory / source.stem
        ir, asm, normal = (stem.with_suffix(s) for s in ['.keel', '.s', '.normal.s'])
        text = run([args.compiler, 'cc', '-O', '--emit-ir', source]).stdout.decode()
        # The text parser currently rejects Darwin import suffixes containing $.
        # Substitute only unquoted global tokens, then restore the exact symbol
        # in assembly. Machine-code parity below remains mandatory.
        aliases = {}
        pieces = re.split(r'("(?:\\.|[^"\\])*")', text)
        def portable(match):
            name = match[0][1:]
            if '$' not in name:
                return match[0]
            if name not in aliases:
                alias = f'__keel_profile_import_{len(aliases)}'
                assert alias not in text
                aliases[name] = alias
            return '@' + aliases[name]
        for index in range(0, len(pieces), 2):
            pieces[index] = re.sub(r'@[A-Za-z_.$][A-Za-z_0-9.$]*', portable, pieces[index])
        ir.write_text(''.join(pieces))
        run([args.compiler, 'compile', '-g', '--opt-tier', 'none', ir, '-o', asm])
        assembly = asm.read_text()
        for name, alias in aliases.items():
            assembly = assembly.replace(alias, name)
        asm.write_text(assembly)
        run([args.compiler, 'cc', '-O', source, '-o', normal])
        obj, plain, reference = (stem.with_suffix(s) for s in ['.o', '.normal.o', '.clang.o'])
        run(['clang', '-c', asm, '-o', obj])
        run(['clang', '-c', normal, '-o', plain])
        assert text_bytes(obj) == text_bytes(plain), f'profiling changed machine code: {source}'
        result = run(['clang', '-O2', '-gline-tables-only', '-ffp-contract=off',
                      '-Rpass=loop-vectorize', '-Rpass-missed=loop-vectorize',
                      '-Rpass-analysis=loop-vectorize', '-c', source, '-o', reference])
        stem.with_suffix('.clang-remarks.txt').write_bytes(result.stderr)
        return {'source': str(source), 'source_sha256': sha(source), 'ir': str(ir),
                'ir_sha256': sha(ir), 'text_sha256': hashlib.sha256(text_bytes(obj)).hexdigest(),
                'normal_text_identical': True, 'object': str(obj), 'normal_object': str(plain),
                'clang_object': str(reference), 'darwin_import_aliases': aliases}

    with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as pool:
        units = list(pool.map(unit, sources))
    binaries = {}
    for variant, key in [('keel', 'object'), ('normal', 'normal_object'), ('clang', 'clang_object')]:
        binary = directory / f'{APPS[app][0]}-{variant}'
        run(['clang', *[u[key] for u in units], '-o', binary, '-lm', '-lpthread',
             *(['-ldl'] if platform.system() == 'Linux' else [])])
        binaries[variant] = str(binary)
    assert text_bytes(binaries['keel']) == text_bytes(binaries['normal']), 'linked .text changed'
    return {'units': units, 'binaries': binaries,
            'binary_sha256': {v: sha(p) for v, p in binaries.items()},
            'linked_text_identical': True}


def invocation(app, binary, fixture, bench):
    if app == 'sqlite':
        return [binary, ':memory:'], fixture.read_bytes()
    return [binary, *(['--stack-size', '4194304'] if app == 'quickjs' else []), str(fixture), bench], None


def mac_summary(path):
    raw = path.read_text()
    total = sum(map(int, re.findall(r'^    (\d+) Thread_', raw, re.M)))
    top = raw.split('Sort by top of stack, same collapsed (when >= 5):', 1)[1].split('Binary Images:', 1)[0]
    rows = []
    for line in top.splitlines():
        match = re.match(r'\s+(.+?)\s+\(in (.+?)\)\s+(\d+)\s*$', line)
        if match:
            rows.append({'symbol': match[1], 'image': match[2], 'samples': int(match[3]),
                         'percent': int(match[3]) / total * 100})
    return {'thread_samples': total, 'top_symbols': rows}


def perf_summary(raw):
    symbols, locations = collections.Counter(), collections.Counter()
    pending = None
    for line in raw.splitlines():
        match = re.match(r'\s*(\d+)\s+([0-9a-f]+)\s+(.+?)\s+\((.+?)\)(.*)', line)
        if match:
            if pending:
                period, symbol, image, location = pending
                locations[(symbol, image, location)] += period
            period, _, symbol, image, location = match.groups()
            name = re.sub(r'\+0x[0-9a-f]+$', '', symbol)
            symbols[(name, image)] += int(period)
            pending = (int(period), symbol, image, location.strip())
        elif pending and re.match(r'\s+.+:\d+\s*$', line):
            pending = (*pending[:3], line.strip())
    if pending:
        period, symbol, image, location = pending
        locations[(symbol, image, location)] += period
    total = sum(symbols.values())
    if not total:
        raise ValueError('no parsed perf samples')
    return {'sampled_cycles': total,
            'top_symbols': [{'symbol': s, 'image': i, 'cycles': c, 'percent': 100*c/total}
                            for (s, i), c in symbols.most_common(40)],
            'top_locations': [{'symbol_offset': s, 'image': i, 'location': l,
                               'cycles': c, 'percent': 100*c/total}
                              for (s, i, l), c in locations.most_common(200)]}


def profile(args, app, details, work):
    rows = {}
    for bench in APPS[app][1]:
        original = ROOT / 'suite' / app / ('bench.sql' if app == 'sqlite' else 'bench.lua' if app == 'lua' else 'bench.js')
        original_command, original_input = invocation(app, details['binaries']['clang'], original, bench)
        reference = run(original_command, input=original_input).stdout
        durations = {}
        for variant in ['normal', 'keel', 'clang']:
            command, stdin = invocation(app, details['binaries'][variant], original, bench)
            started = time.perf_counter()
            result = run(command, input=stdin)
            durations[variant] = time.perf_counter() - started
            if result.stdout != reference:
                raise ValueError(f'output mismatch: {app}/{bench}/{variant}')
        row = {'single_run_seconds_not_a_benchmark': durations,
               'expected_stdout': reference.decode(), 'correctness_matches_clang': True, 'profiles': []}
        rows[bench] = row
        for variant in args.variants:
            repeats = max(2, math.ceil((args.seconds * 1.5 + 2) / durations[variant]))
            fixture = work / app / f'{bench}-{variant}-repeat{original.suffix}'
            source = original.read_text()
            if app == 'lua':
                fixture.write_text(f'for __profile_repeat=1,{repeats} do\n{source}\nend\n')
            elif app == 'quickjs':
                fixture.write_text(f'for(let __profile_repeat=0;__profile_repeat<{repeats};__profile_repeat++){{\n{source}\n}}\n')
            else:
                fixture.write_text(('DROP TABLE IF EXISTS t;\n' + source + '\n') * repeats)
            command, stdin = invocation(app, details['binaries'][variant], fixture, bench)
            for round_index in range(args.rounds):
                prefix = work / app / f'{bench}-{variant}-{round_index}'
                if platform.system() == 'Darwin':
                    stdout_path = prefix.with_suffix('.stdout')
                    with stdout_path.open('wb') as stdout:
                        stdin_handle = fixture.open('rb') if stdin else None
                        process = subprocess.Popen(command, stdin=stdin_handle,
                                                   stdout=stdout, stderr=subprocess.PIPE)
                        if stdin_handle:
                            stdin_handle.close()
                        try:
                            time.sleep(.15)
                            profile_path = prefix.with_suffix('.sample.txt')
                            sampler = ['sample', str(process.pid), str(args.seconds), '1', '-fullPaths', '-file', profile_path]
                            run(sampler)
                            error = process.stderr.read()
                            code = process.wait(timeout=120)
                            if code:
                                raise RuntimeError(error.decode(errors='replace'))
                        finally:
                            if process.poll() is None:
                                process.kill(); process.wait()
                    assert stdout_path.read_bytes() == reference * repeats, f'repeated output mismatch: {prefix}'
                    summary = mac_summary(profile_path)
                else:
                    profile_path = prefix.with_suffix('.perf.data')
                    sampler = ['sudo', '-n', 'perf', 'record', '-q', '-e', args.event, '-F', '499',
                               '-o', profile_path, '--', 'taskset', '-c', str(args.cpu), *command]
                    result = run(sampler, input=stdin)
                    assert result.stdout == reference * repeats, f'repeated output mismatch: {prefix}'
                    prefix.with_suffix('.stdout').write_bytes(result.stdout)
                    raw = run(['sudo', '-n', 'perf', 'script', '-i', profile_path,
                               '-F', 'period,ip,sym,symoff,dso,srcline']).stdout.decode()
                    profile_path = prefix.with_suffix('.perf.txt')
                    profile_path.write_text(raw)
                    summary = perf_summary(raw)
                row['profiles'].append({'variant': variant, 'round': round_index, 'repetitions': repeats,
                                        'command': command, 'sample_command': list(map(str, sampler)),
                                        'profile': str(profile_path), 'profile_sha256': sha(profile_path), **summary})
                print(f'profiled {app}/{bench}/{variant}/{round_index}', flush=True)
    return rows


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--compiler', type=pathlib.Path, required=True)
    parser.add_argument('--prepared', type=pathlib.Path, default=ROOT / 'target/suite')
    parser.add_argument('--work-dir', type=pathlib.Path, required=True)
    parser.add_argument('--suites', nargs='+', choices=APPS, default=list(APPS))
    parser.add_argument('--variants', nargs='+', choices=['keel', 'clang'], default=['keel'])
    parser.add_argument('--seconds', type=int, default=6)
    parser.add_argument('--rounds', type=int, default=2)
    parser.add_argument('--jobs', type=int, default=4)
    parser.add_argument('--cpu', type=int, default=0)
    parser.add_argument('--event', default='cpu_core/cycles/u')
    parser.add_argument('--reuse-builds', action='store_true')
    parser.add_argument('--build-only', action='store_true')
    args = parser.parse_args()
    args.compiler = args.compiler.resolve(); args.prepared = args.prepared.resolve()
    args.work_dir = args.work_dir.resolve(); args.work_dir.mkdir(parents=True, exist_ok=True)
    if not 1 <= args.seconds <= 30 or not 1 <= args.rounds <= 5:
        parser.error('use 1..30 seconds and 1..5 rounds')
    output = args.work_dir / 'profile.json'
    report = json.loads(output.read_text()) if args.reuse_builds else {
        'host': platform.platform(), 'compiler_sha256': sha(args.compiler), 'apps': {},
        'method': 'Optimized C IR then line tables for printed IR; exact .text object parity with ordinary cc -O required. Clang is O2 with line tables and FP contraction disabled. Single-run durations only calibrate repetitions, not performance claims. macOS profiles use wall-clock stack sampling; Linux uses weighted cycle samples on one P core. Repeated workload stdout must match the single Clang result each iteration. Profiling is separate from latency measurement.'}
    report['profiler_script_sha256'] = sha(__file__)
    for app in args.suites:
        if not args.reuse_builds or app not in report['apps']:
            report['apps'][app] = build(args, app, args.work_dir)
            output.write_text(json.dumps(report, indent=2) + '\n')
            print(f'built and verified {app}', flush=True)
        if not args.build_only:
            report['apps'][app]['workloads'] = profile(args, app, report['apps'][app], args.work_dir)
        output.write_text(json.dumps(report, indent=2) + '\n')


if __name__ == '__main__':
    main()
