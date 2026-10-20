#!/usr/bin/env python3
"""Native application benchmark: frozen inputs, correctness first, paired timings.

Compilation is serial preprocessed C -> assembly and C -> object, separately.
Keel's object boundary uses its own emitter; other compilers use -c. Preprocessing,
linking, compiler construction and dependency setup are excluded from both.
Runtime is CLI wall time including startup. No LTO, native CPU tuning or PGO.
"""
import argparse
import copy
import gzip
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import random
import re
import shlex
import shutil
import statistics
import subprocess
import time
try:
    import tomllib
except ImportError:
    from pip._vendor import tomli as tomllib

APPS = ['lua', 'quickjs', 'sqlite']


def common_float_aliases(text):
    """Rename explicit Clang/glibc compatibility typedefs, not their types.

    Clang's preprocessor exposes `typedef float _Float32` in older glibc
    headers. GCC reserves that identifier as a keyword and rejects the same
    preprocessed declaration. Renaming the existing alias and its identifier
    uses preserves the Clang-preprocessed type binding for every compiler.
    Strings/comments remain untouched. No application operations are rewritten.
    """
    names = ['_Float32', '_Float64', '_Float32x', '_Float64x']
    aliases = {n: 'keel_bench_compat' + n for n in names
               if re.search(r'\btypedef\s+(?:float|double|long\s+double)\s+' + n + r'\s*;', text)}
    token = r'/\*[\s\S]*?\*/|//[^\n]*|"(?:\\[\s\S]|[^"\\])*"|\'(?:\\[\s\S]|[^\'\\])*\'|[A-Za-z_][A-Za-z_0-9]*'
    return re.sub(token, lambda m: aliases.get(m[0], m[0]), text), aliases


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def interval(values):
    rng = random.Random(24517000)
    boot = sorted(statistics.median(rng.choices(values, k=len(values))) for _ in range(4000))
    return {'median': statistics.median(values), 'bootstrap95': [boot[99], boot[3899]],
            'pairs': len(values)}


def noise(cpu):
    out = {'load': os.getloadavg(), 'at_ns': time.time_ns()}
    if Path('/proc/stat').exists():
        rows = Path('/proc/stat').read_text().splitlines()
        for name in ['cpu', 'cpu' + str(cpu)]:
            row = next((r.split() for r in rows if r.startswith(name + ' ')), None)
            if row:
                out[name] = list(map(int, row[1:]))
        out['pressure'] = Path('/proc/pressure/cpu').read_text() if Path('/proc/pressure/cpu').exists() else None
    return out


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--root', type=Path, required=True)
    p.add_argument('--work', type=Path, required=True)
    p.add_argument('--keel', type=Path)
    p.add_argument('--clang', default='clang')
    p.add_argument('--gcc')
    p.add_argument('--apps', nargs='+', choices=APPS)
    p.add_argument('--configs', nargs='+', help='selected named configurations; must include clang-O2')
    p.add_argument('--gcc-native-preprocess', action='store_true',
                   help='supplementary GCC source-level comparison using its native headers/builtins')
    p.add_argument('--cpu', type=int)
    p.add_argument('--prepare-only', action='store_true')
    p.add_argument('--reuse-prepared', action='store_true')
    p.add_argument('--normalize-prepared', action='store_true',
                   help='preserve raw .i files and rename explicit glibc float aliases for common GCC/Clang inputs')
    p.add_argument('--compile-rounds', type=int, default=5)
    p.add_argument('--runtime-rounds', type=int, default=7)
    args = p.parse_args()
    chosen_apps = args.apps or APPS
    root, work = args.root.resolve(), args.work.resolve()
    work.mkdir(parents=True, exist_ok=False)
    clang = str(Path(shutil.which(args.clang) or args.clang).resolve())
    gcc = str(Path(shutil.which(args.gcc) or args.gcc).resolve()) if args.gcc else None
    prefix = ['taskset', '-c', str(args.cpu)] if args.cpu is not None else []
    report = {'complete': False, 'phase': 'prepare', 'host': platform.node(),
              'platform': platform.platform(), 'arch': platform.machine(), 'cpu_affinity': args.cpu,
              'runner_sha256': sha(__file__), 'method': __doc__, 'started_utc': time.strftime('%FT%TZ', time.gmtime()),
              'apps': {}, 'errors': [], 'noise_before': noise(args.cpu), 'compiler_versions': {},
              'compile_rounds': args.compile_rounds, 'runtime_rounds': args.runtime_rounds,
              'compile_warmups': 1, 'runtime_warmups': 2}
    log = gzip.open(work / 'commands.jsonl.gz', 'wt')

    def save():
        temp = work / 'report.json.tmp'
        temp.write_text(json.dumps(report, indent=2) + '\n')
        temp.replace(work / 'report.json')
        log.flush()

    def run(argv, label, cwd=None, stdin=None, timeout=600, check=True, pinned=True):
        argv = list(map(str, argv))
        command = prefix + argv if pinned else argv
        start = time.perf_counter_ns()
        try:
            proc = subprocess.run(command, cwd=cwd, input=stdin, capture_output=True, timeout=timeout)
            seconds = (time.perf_counter_ns() - start) / 1e9
            row = {'argv': command, 'cwd': str(cwd) if cwd else None, 'label': label,
                   'seconds': seconds, 'returncode': proc.returncode,
                   'stdout': proc.stdout.decode(errors='replace'), 'stderr': proc.stderr.decode(errors='replace'),
                   'stdin_sha256': hashlib.sha256(stdin).hexdigest() if stdin is not None else None}
            log.write(json.dumps(row) + '\n')
            if check and proc.returncode:
                raise RuntimeError(row)
            return proc, seconds
        except subprocess.TimeoutExpired:
            log.write(json.dumps({'argv': command, 'label': label, 'timeout': timeout}) + '\n')
            raise

    def expand(text, app, binary=None, gen=None):
        return text.replace('{suite}', str(root / 'suite' / app)).replace('{work}', str(root / 'target/suite' / app)).replace('{bin}', str(binary)).replace('{gen}', str(gen))

    def spec(app):
        m = tomllib.loads((root / 'suite' / app / 'suite.toml').read_text())
        b = copy.deepcopy(m['build'])
        host_os = {'Darwin': 'macos', 'Linux': 'linux'}.get(platform.system(), platform.system().lower())
        for extra in b.get('when', []):
            if extra.get('os', host_os) != host_os:
                continue
            arch = 'aarch64' if platform.machine() == 'arm64' else platform.machine()
            if extra.get('arch', arch) != arch:
                continue
            for k in ['defines', 'includes', 'libs']:
                b.setdefault(k, []).extend(extra.get(k, []))
        return m, b

    try:
        report['compiler_versions']['clang'] = run([clang, '--version'], 'toolchain', pinned=False)[0].stdout.decode()
        if gcc:
            report['compiler_versions']['gcc'] = run([gcc, '--version'], 'toolchain', pinned=False)[0].stdout.decode()
        if platform.system() == 'Linux':
            report['lscpu'] = run(['lscpu'], 'host', pinned=False)[0].stdout.decode()
            report['glibc'] = run(['getconf', 'GNU_LIBC_VERSION'], 'host', pinned=False)[0].stdout.decode().strip()
            report['cpu_siblings'] = Path(f'/sys/devices/system/cpu/cpu{args.cpu}/topology/thread_siblings_list').read_text().strip() if args.cpu is not None else None
        else:
            report['cpu'] = run(['sysctl', '-n', 'machdep.cpu.brand_string'], 'host', pinned=False)[0].stdout.decode().strip()
        all_sources = {}
        for app in APPS:
            m, b = spec(app)
            prepared = root / 'target/suite' / app
            prepared.mkdir(parents=True, exist_ok=True)
            marker = prepared / 'benchmark-prepared.json'
            if not args.reuse_prepared:
                fetches = {}
                for f in m['fetch']:
                    archive = prepared / f['url'].split('/')[-1]
                    if not archive.exists():
                        run(['curl', '-fsSL', f['url'], '-o', archive], 'fetch', pinned=False)
                    assert sha(archive) == f['sha256'], archive
                    fetches[archive.name] = sha(archive)
                    if archive.suffix == '.zip':
                        run(['unzip', '-q', '-o', archive, '-d', prepared], 'unpack', pinned=False)
                    else:
                        run(['tar', '-xf', archive, '-C', prepared], 'unpack', pinned=False)
                directory = prepared / b['dir']
                gen = prepared / 'gen'
                gen.mkdir(exist_ok=True)
                defines = ['-D' + s for s in b.get('defines', [])]
                includes = ['-I' + s for s in b.get('includes', [])]
                libs = ['-l' + s for s in b.get('libs', [])]
                if app == 'quickjs':
                    run([clang, '-w', '-O2', *defines, 'qjsc.c', 'quickjs.c', 'libregexp.c', 'libunicode.c', 'cutils.c', 'quickjs-libc.c', 'libbf.c', '-o', gen / 'qjsc', *libs], 'prepare-qjsc', cwd=directory)
                    run([gen / 'qjsc', '-c', '-o', gen / 'repl.c', '-m', 'repl.js'], 'prepare-repl', cwd=directory)
                    run([gen / 'qjsc', '-fbignum', '-c', '-o', gen / 'qjscalc.c', 'qjscalc.js'], 'prepare-calculator', cwd=directory)
                files = []
                for pattern in b['sources']:
                    pattern = expand(pattern, app, gen=gen)
                    matches = [Path(pattern)] if Path(pattern).is_absolute() else sorted(directory.glob(pattern))
                    files.extend(f for f in matches if f.name not in b.get('exclude', []))
                pre = prepared / 'pre'
                pre.mkdir(exist_ok=True)
                for file in files:
                    run([clang, '-E', '-std=' + b['std'], *defines, *includes, file, '-o', pre / (file.stem + '.i')], 'preprocess', cwd=directory)
                marker.write_text(json.dumps({'os': platform.system(), 'arch': platform.machine(), 'clang': report['compiler_versions']['clang'], 'fetches': fetches, 'sources': {p.name: sha(p) for p in sorted(pre.glob('*.i'))}}, indent=2) + '\n')
            prep = json.loads(marker.read_text())
            assert prep['os'] == platform.system() and prep['arch'] == platform.machine(), prep
            sources = sorted((prepared / 'pre').glob('*.i'))
            assert {p.name: sha(p) for p in sources} == prep['sources'], app
            if args.normalize_prepared:
                backup = prepared / 'pre-raw'
                backup.mkdir(exist_ok=True)
                mapping = {}
                for source in sources:
                    raw = backup / source.name
                    if not raw.exists():
                        shutil.copy2(source, raw)
                    normalized, aliases = common_float_aliases(raw.read_text())
                    source.write_text(normalized)
                    mapping[source.name] = aliases
                prep['raw_sources'] = {p.name: sha(p) for p in sorted(backup.glob('*.i'))}
                prep['compatibility_alias_renames'] = mapping
                prep['sources'] = {p.name: sha(p) for p in sources}
                marker.write_text(json.dumps(prep, indent=2) + '\n')
            all_sources[app] = sources
            report['apps'][app] = {'manifest_sha256': sha(root / 'suite' / app / 'suite.toml'), 'preparation': prep, 'sources': {p.name: sha(p) for p in sources}, 'builds': {}, 'checks': {}, 'compile': {}, 'runtime': {}}
        assert [len(all_sources[a]) for a in APPS] == [33, 9, 2]
        if args.prepare_only:
            report['complete'] = True
            report['phase'] = 'prepared'
            save()
            return
        assert args.keel
        report['apps'] = {a: report['apps'][a] for a in chosen_apps}
        keel = str(args.keel.resolve())
        configs = {'keel-none': ('keel', keel, []),
                   'keel-fast': ('keel', keel, ['--opt-tier', 'fast', '--isel-tier', 'full', '--regalloc', 'full']),
                   'keel-full': ('keel', keel, ['-O']),
                   'clang-O0': ('cc', clang, ['-O0']), 'clang-O2': ('cc', clang, ['-O2'])}
        if gcc:
            configs.update({'gcc-O0': ('cc', gcc, ['-O0']), 'gcc-O2': ('cc', gcc, ['-O2'])})
        if args.configs:
            assert 'clang-O2' in args.configs and set(args.configs) <= configs.keys()
            configs = {n: c for n, c in configs.items() if n in args.configs}
        gcc_sources = {}
        if args.gcc_native_preprocess:
            assert gcc
            for app in chosen_apps:
                _, b = spec(app)
                prepared = root / 'target/suite' / app
                directory, gen = prepared / b['dir'], prepared / 'gen'
                dest = work / 'gcc-prepared' / app
                dest.mkdir(parents=True)
                files = []
                for pattern in b['sources']:
                    pattern = expand(pattern, app, gen=gen)
                    matches = [Path(pattern)] if Path(pattern).is_absolute() else sorted(directory.glob(pattern))
                    files.extend(f for f in matches if f.name not in b.get('exclude', []))
                for source in files:
                    run([gcc, '-E', '-std=' + b['std'], *['-D' + d for d in b.get('defines', [])], *['-I' + i for i in b.get('includes', [])], source, '-o', dest / (source.stem + '.i')], 'gcc-native-preprocess', cwd=directory)
                gcc_sources[app] = {p.name: p for p in dest.glob('*.i')}
                assert gcc_sources[app].keys() == {p.name for p in all_sources[app]}
                report['apps'][app]['gcc_native_sources'] = {n: sha(p) for n, p in gcc_sources[app].items()}
            report['gcc_input_scope'] = 'Native GCC preprocessing of identical upstream C and generated sources/defines; not byte-identical Clang-preprocessed inputs. Preprocessing excluded from timings.'
        report['configs'] = {n: {'kind': k, 'compiler': c, 'flags': flags} for n, (k, c, flags) in configs.items()}
        report['compiler_sha256'] = {c: sha(c) for _, c, _ in configs.values()}
        report['phase'] = 'build'
        save()

        def compile_one(app, name, source, boundary, label):
            kind, compiler, flags = configs[name]
            if name.startswith('gcc-') and args.gcc_native_preprocess:
                source = gcc_sources[app][source.name]
            suffix = '.s' if boundary == 'assembly' else '.o'
            out = work / app / name / (source.stem + suffix)
            if kind == 'keel':
                argv = [compiler, 'cc', *flags, *(['--emit-object'] if boundary == 'object' else []), source, '-o', out]
            else:
                _, build_spec = spec(app)
                arch = ['-march=x86-64'] if platform.machine() == 'x86_64' else ['-march=armv8-a']
                argv = [compiler, '-w', '-std=' + build_spec['std'], '-ffp-contract=off', *arch, *flags, '-S' if boundary == 'assembly' else '-c', source, '-o', out]
            return run(argv, label)[1]

        # Build everything before tests/timing, retaining unsuccessful cases.
        for app in chosen_apps:
            _, b = spec(app)
            for name in configs:
                dest = work / app / name
                dest.mkdir(parents=True)
                try:
                    for source in all_sources[app]:
                        compile_one(app, name, source, 'object', 'initial-build')
                    binary = dest / b['binary']
                    run([clang, *[dest / (s.stem + '.o') for s in all_sources[app]], '-o', binary, *['-l' + s for s in b.get('libs', [])]], 'common-link')
                    report['apps'][app]['builds'][name] = {'status': 'built', 'binary': str(binary), 'sha256': sha(binary), 'bytes': binary.stat().st_size}
                except Exception as e:
                    report['apps'][app]['builds'][name] = {'status': 'failed', 'error': str(e)}
                    report['errors'].append({'app': app, 'config': name, 'phase': 'build', 'error': str(e)})
                print('build', app, name, report['apps'][app]['builds'][name]['status'], flush=True)
                save()
        report['phase'] = 'correctness'

        def invocation(app, bench, binary):
            m, _ = spec(app)
            definition = next(b for b in m['bench'] if b['name'] == bench)
            argv = shlex.split(expand(definition['run'], app, binary))
            stdin = Path(expand(definition['stdin'], app)).read_bytes() if definition.get('stdin') else None
            return argv, stdin

        for app in chosen_apps:
            detail = report['apps'][app]
            m, b = spec(app)
            reference = detail['builds'].get('clang-O2', {})
            if reference.get('status') != 'built':
                raise RuntimeError('Clang O2 reference failed for ' + app)
            reference_tests = None
            for name in ['clang-O2', *[n for n in configs if n != 'clang-O2']]:
                build = detail['builds'][name]
                if build['status'] != 'built':
                    continue
                binary = Path(build['binary'])
                try:
                    test_dir = work / app / name / 'tests'
                    cwd = work / app / name
                    if app == 'lua':
                        shutil.copytree(root / 'target/suite/lua/lua-5.4.7-tests', test_dir)
                        proc, _ = run([binary, '-e', '_U=true', 'all.lua'], 'suite-test', cwd=test_dir)
                        outputs = [proc.stdout]
                    elif app == 'quickjs':
                        shutil.copytree(root / 'target/suite/quickjs/quickjs-2024-01-13/tests', test_dir)
                        outputs = []
                        for command in m['test']['run']:
                            argv = shlex.split(expand(command, app, binary))
                            proc, _ = run(argv, 'suite-test', cwd=cwd)
                            outputs.append(proc.stdout)
                    else:
                        proc, _ = run([binary, ':memory:'], 'suite-test', stdin=(root / 'suite/sqlite/queries.sql').read_bytes())
                        outputs = [proc.stdout]
                        if reference_tests is None:
                            reference_tests = outputs
                        assert outputs == reference_tests, 'SQLite query output differs'
                    detail['checks'][name] = {'suite_test_passed': True, 'commands': len(outputs)}
                    for bench in m['bench']:
                        argv, stdin = invocation(app, bench['name'], binary)
                        proc, _ = run(argv, 'benchmark-preflight', stdin=stdin)
                        ref_argv, ref_stdin = invocation(app, bench['name'], Path(reference['binary']))
                        expected, _ = run(ref_argv, 'reference-preflight', stdin=ref_stdin)
                        assert proc.stdout == expected.stdout, (app, bench['name'], name, 'benchmark output mismatch')
                        detail.setdefault('expected', {})[bench['name']] = proc.stdout.decode()
                    detail['checks'][name]['benchmark_outputs_passed'] = True
                except Exception as e:
                    build['status'] = 'incorrect'
                    detail['checks'][name] = {'passed': False, 'error': str(e)}
                    report['errors'].append({'app': app, 'config': name, 'phase': 'correctness', 'error': str(e)})
                print('check', app, name, build['status'], flush=True)
                save()
        report['phase'] = 'compile'
        rng = random.Random(24517000)
        for app in chosen_apps:
            detail = report['apps'][app]
            names = [n for n in configs if detail['builds'][n]['status'] == 'built']
            for boundary in ['assembly', 'object']:
                detail['compile'][boundary] = []
                for block in range(-1, args.compile_rounds):
                    order = names.copy()
                    rng.shuffle(order)
                    before = noise(args.cpu)
                    row = {'block': block, 'warmup': block < 0, 'order': order, 'seconds': {}, 'noise_before': before}
                    for name in order:
                        row['seconds'][name] = sum(compile_one(app, name, s, boundary, 'compile-timing') for s in all_sources[app])
                    row['noise_after'] = noise(args.cpu)
                    detail['compile'][boundary].append(row)
                    save()
                    print('compile', app, boundary, block, flush=True)
        report['phase'] = 'runtime'
        for app in chosen_apps:
            detail = report['apps'][app]
            m, _ = spec(app)
            names = [n for n in configs if detail['builds'][n]['status'] == 'built']
            # An interleaved identical executable control measures placement/noise,
            # without pretending it is an independent compiler configuration.
            control = Path(detail['builds']['clang-O2']['binary']).with_name('baseline-copy')
            shutil.copy2(detail['builds']['clang-O2']['binary'], control)
            assert sha(control) == detail['builds']['clang-O2']['sha256']
            for bench in m['bench']:
                rows = []
                detail['runtime'][bench['name']] = rows
                for block in range(-2, args.runtime_rounds):
                    order = [*names, 'identical-control']
                    rng.shuffle(order)
                    row = {'block': block, 'warmup': block < 0, 'order': order, 'seconds': {}, 'noise_before': noise(args.cpu)}
                    for name in order:
                        binary = control if name == 'identical-control' else Path(detail['builds'][name]['binary'])
                        argv, stdin = invocation(app, bench['name'], binary)
                        proc, seconds = run(argv, 'runtime-timing', stdin=stdin)
                        assert proc.stdout.decode() == detail['expected'][bench['name']], (app, bench['name'], name)
                        row['seconds'][name] = seconds
                    row['noise_after'] = noise(args.cpu)
                    rows.append(row)
                    save()
                    print('runtime', app, bench['name'], block, flush=True)
        for app in chosen_apps:
            detail = report['apps'][app]
            detail['summary'] = {'compile': {}, 'runtime': {}}
            for stage, batches in [('compile', detail['compile']), ('runtime', detail['runtime'])]:
                for boundary, rows in batches.items():
                    rows = [r for r in rows if not r['warmup']]
                    detail['summary'][stage][boundary] = {
                        name: {'seconds_median': statistics.median(r['seconds'][name] for r in rows),
                               'time_over_clang_O2': interval([r['seconds'][name] / r['seconds']['clang-O2'] for r in rows])}
                        for name in rows[0]['seconds']}
            for name, build in detail['builds'].items():
                if build['status'] in ['built', 'incorrect']:
                    assert sha(build['binary']) == build['sha256']
            assert {p.name: sha(p) for p in all_sources[app]} == detail['sources']
        assert all(sha(p) == digest for p, digest in report['compiler_sha256'].items())
        report['noise_after'] = noise(args.cpu)
        report['phase'] = 'complete'
        report['complete'] = True
        save()
        print('COMPLETE', 'errors', len(report['errors']), flush=True)
    except Exception as e:
        report['fatal_error'] = str(e)
        save()
        raise
    finally:
        log.close()


if __name__ == '__main__':
    main()
