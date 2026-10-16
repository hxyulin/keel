#!/usr/bin/env python3
"""Evidence-only 2x2 validation. No profiles, application edits or host changes.

Each invocation runs one explicit phase. Runtime and CLI timing must be
serialized across hosts by the operator. Inputs/references are read-only copies
from the previously audited current-app-runtime campaign.
"""
import argparse
import gzip
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import random
import re
import shutil
import statistics as st
import subprocess
import time

from profile_app_loops import APPS, invocation, text_bytes

CONFIGS = ['before-default', 'after-default', 'before-all', 'after-all']
CONTRASTS = {'primary': ('after-default', 'before-default'),
             'all': ('after-all', 'before-all'),
             'rollout': ('after-all', 'after-default'),
             'baseline_all': ('before-all', 'before-default')}
REVISIONS = {'before': '56713b01b6fffac758c1aef378260778b578766e',
             'after': 'e85816ff651e912580bb8bd800c6c4884047a73d'}
MAC = platform.system() == 'Darwin'


def sha(p):
    return hashlib.sha256(Path(p).read_bytes()).hexdigest()


def save(p, value):
    p.write_text(json.dumps(value, indent=2) + '\n')


def run(argv, *, cwd=None, input=None, required=True, timeout=900):
    argv = list(map(str, argv))
    p = subprocess.run(argv, cwd=cwd, input=input, capture_output=True, timeout=timeout)
    with (E / 'commands.jsonl').open('a') as f:
        f.write(json.dumps({'argv': argv, 'cwd': str(cwd or Path.cwd()),
                            'exit': p.returncode, 'stdout': p.stdout.decode(errors='replace'),
                            'stderr': p.stderr.decode(errors='replace')}) + '\n')
    if required and p.returncode:
        raise RuntimeError((argv, p.returncode, p.stderr.decode(errors='replace')))
    return p


def boundary(label):
    commands = [['date', '-u'], ['uptime'],
                ['ps', '-Ao', 'pid,ppid,pcpu,comm'] if MAC else
                ['ps', '-eo', 'pid,ppid,pcpu,comm', '--sort=-pcpu']]
    if MAC:
        commands += [['sysctl', 'hw.model', 'hw.ncpu', 'hw.memsize'], ['pmset', '-g', 'therm']]
    else:
        commands += [['taskset', '-pc', str(os.getpid())], ['lscpu'],
                     ['sh', '-c', 'for f in /sys/devices/system/cpu/cpu2/cpufreq/* /sys/devices/system/cpu/cpu2/topology/thread_siblings_list; do test -f "$f" && printf "%s " "$f" && cat "$f"; done']]
    out = []
    for cmd in commands:
        p = run(cmd, required=False)
        out.append({'argv': cmd, 'exit': p.returncode,
                    'stdout': p.stdout.decode(errors='replace'), 'stderr': p.stderr.decode(errors='replace')})
    save(E / ('boundary-' + label + '.json'), out)


def compiler(config):
    return B / (config.split('-')[0] + '-target') / 'release/keel'


def flags(config):
    return ['cc', '-O'] + (['--isel', 'all'] if config.endswith('-all') else [])


def verify_compilers():
    result = {}
    for v, rev in REVISIONS.items():
        source = B / (v + '-source')
        assert run(['git', '-C', source, 'rev-parse', 'HEAD']).stdout.decode().strip() == rev
        assert not run(['git', '-C', source, 'status', '--porcelain']).stdout.strip()
        binary = compiler(v + '-default')
        fingerprints = {}
        for p in (B / (v + '-target') / 'release/.fingerprint').glob('keel-*/bin-keel.json'):
            fingerprints[str(p)] = json.loads(p.read_text())
        assert fingerprints
        assert all(x['features'] == '[]' and x['rustflags'] == [] for x in fingerprints.values())
        result[v] = {'revision': rev, 'source': str(source), 'binary': str(binary),
                     'sha256': sha(binary), 'bytes': binary.stat().st_size,
                     'crates_tree': run(['git', '-C', source, 'rev-parse', 'HEAD:crates']).stdout.decode().strip(),
                     'fingerprints': fingerprints,
                     'build_argv': ['cargo', 'build', '--locked', '--release', '-p', 'keel', '--no-default-features', '-j', '1']}
    if (E / 'provenance.json').exists():
        old = json.loads((E / 'provenance.json').read_text())['compilers']
        assert result == old
    return result


def prepare():
    old = json.loads((P / 'provenance.json').read_text())
    inputs = {}
    for app, count in [('lua', 33), ('quickjs', 9), ('sqlite', 2)]:
        sources = sorted((P / 'inputs' / app / 'pre').glob('*.i'))
        assert len(sources) == count
        dest = E / 'inputs' / app / 'pre'; dest.mkdir(parents=True)
        for src in sources:
            expected = [v['sha256'] for k, v in old['inputs'].items() if k.endswith('/' + app + '/pre/' + src.name)]
            assert expected == [sha(src)]
            shutil.copy2(src, dest / src.name)
            inputs[str(dest / src.name)] = {'prior': str(src), 'sha256': sha(src)}
    for name in ['fixtures', 'lua-tests', 'quickjs-tests']:
        shutil.copytree(P / name, E / name)
    fixtures = {}
    for src in sorted((E / 'fixtures').rglob('*')):
        if src.is_file():
            suffix = str(src.relative_to(E))
            expected = [v for k, v in old['fixtures'].items() if k.endswith('/' + suffix)]
            assert expected == [sha(src)], suffix
            fixtures[suffix] = sha(src)
    tests = {}
    for name in ['lua-tests', 'quickjs-tests']:
        for src in sorted((E / name).rglob('*')):
            if src.is_file():
                rel = str(src.relative_to(E))
                assert old['test_inputs'][rel] == sha(src), rel
                tests[rel] = sha(src)
    save(E / 'provenance.json', {'compilers': verify_compilers(), 'inputs': inputs,
         'fixtures': fixtures, 'test_inputs': tests, 'prior_provenance_sha256': sha(P / 'provenance.json'),
         'host': platform.platform(), 'node': platform.node(),
         'rustc': run(['rustc', '-Vv']).stdout.decode(), 'cargo': run(['cargo', '-V']).stdout.decode(),
         'clang': run(['clang', '--version']).stdout.decode(), 'python': platform.python_version(),
         'harness_sha256': sha(__file__),
         'dependencies_sha256': {'profile_app_loops.py': sha(Path(__file__).parent / 'profile_app_loops.py')},
         'harness_revision': run(['git', '-C', Path(__file__).parent, 'rev-parse', 'HEAD'], required=False).stdout.decode().strip(),
         'environment': dict(sorted(os.environ.items() & { (k, v) for k, v in os.environ.items() if k in
              ['PATH', 'LANG', 'LC_ALL', 'LC_CTYPE', 'TZ', 'LD_LIBRARY_PATH', 'DYLD_LIBRARY_PATH', 'MACOSX_DEPLOYMENT_TARGET', 'OMP_NUM_THREADS', 'MALLOC_CONF', 'GLIBC_TUNABLES', 'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS'] })),
         'method': 'No optional Cargo features/rustflags; report OFF; AVX OFF; cc -O emits assembly; system clang assembles/links only. No LTO/PGO/native CPU/FMA/fastmath changes. CPU2 runtime/compile subprocess affinity on LinuxHost; macOS scheduler, no pin claim. Two warmup and twelve measured complete blocks; rotated and reversed order. Copy/hash canonical binary before timer. No samples discarded. Bootstrap seed42, 10000 paired block resamples.'})


def file_identity(p):
    text = text_bytes(p)
    return {'path': str(p), 'sha256': sha(p), 'file_bytes': p.stat().st_size,
            'text_bytes': len(text), 'text_sha256': hashlib.sha256(text).hexdigest()}


def build():
    verify_compilers()
    result = {}
    for c in CONFIGS:
        result[c] = {}
        for app in APPS:
            d = E / 'apps' / c / app; d.mkdir(parents=True)
            units = []
            for src in sorted((E / 'inputs' / app / 'pre').glob('*.i')):
                asm, obj = d / (src.stem + '.s'), d / (src.stem + '.o')
                run([compiler(c), *flags(c), src, '-o', asm])
                run(['clang', '-c', asm, '-o', obj])
                units.append({'source': str(src), 'source_sha256': sha(src), 'assembly_sha256': sha(asm),
                              **file_identity(obj)})
            binary = d / APPS[app][0]
            run(['clang', *[u['path'] for u in units], '-o', binary, '-lm', '-lpthread', *([] if MAC else ['-ldl'])])
            result[c][app] = {'units': units, 'binary': file_identity(binary),
                             'object_text_bytes': sum(u['text_bytes'] for u in units),
                             'object_file_bytes': sum(u['file_bytes'] for u in units)}
            save(E / 'build.json', result)
            print('built', c, app, flush=True)


def fixture(app):
    return E / 'fixtures' / app / ('bench.lua' if app == 'lua' else 'bench.js' if app == 'quickjs' else 'bench.sql')


def check():
    build = json.loads((E / 'build.json').read_text())
    rows = []
    for c in CONFIGS:
        for app in APPS:
            binary = build[c][app]['binary']['path']
            for bench in APPS[app][1]:
                cmd, stdin = invocation(app, binary, fixture(app), bench)
                p = run(cmd, input=stdin)
                ref = P / app / (bench + '-clang.stdout')
                assert p.stdout == ref.read_bytes(), (c, app, bench)
                rows.append({'config': c, 'app': app, 'test': bench, 'passed': True, 'reference_sha256': sha(ref)})
            if app == 'lua':
                d = E / ('lua-tests-' + c); shutil.copytree(E / 'lua-tests', d)
                run([binary, '-e', '_U=true', 'all.lua'], cwd=d)
                rows.append({'config': c, 'app': app, 'test': 'portable', 'passed': True})
            elif app == 'quickjs':
                for test in ['test_closure', 'test_language', 'test_builtin', 'test_loop', 'test_std', 'test_worker', 'test_op_overloading', 'test_bignum', 'test_qjscalc']:
                    f = ['--qjscalc'] if test == 'test_qjscalc' else ['--bignum'] if test in ['test_op_overloading', 'test_bignum'] else []
                    run([binary, *f, 'tests/' + test + '.js'], cwd=E / 'quickjs-tests')
                    rows.append({'config': c, 'app': app, 'test': test, 'passed': True})
            else:
                p = run([binary, ':memory:'], input=(E / 'fixtures/sqlite/queries.sql').read_bytes())
                ref = P / app / 'tests-clang.log'
                assert p.stdout == ref.read_bytes()
                rows.append({'config': c, 'app': app, 'test': 'queries.sql', 'passed': True, 'reference_sha256': sha(ref)})
            save(E / 'checks.json', rows)
            print('checked', c, app, flush=True)


def order(block):
    # Eight-block balanced cycle: every position, forward and reverse.
    base = CONFIGS if (block // 4) % 2 == 0 else list(reversed(CONFIGS))
    k = block % 4
    return base[k:] + base[:k]


def describe(xs):
    med = st.median(xs)
    return {'median': med, 'mad': st.median(abs(x-med) for x in xs), 'range': [min(xs), max(xs)]}


def summarize(samples):
    out = {'samples_ms': samples, 'latency_ms': {k: describe(v) for k, v in samples.items()}, 'contrasts': {}}
    for label, (num, den) in CONTRASTS.items():
        ratios = [a/b for a, b in zip(samples[num], samples[den])]
        rng = random.Random(42)
        boots = sorted(st.median(rng.choices(ratios, k=len(ratios))) for _ in range(10000))
        out['contrasts'][label] = {**describe(ratios), 'paired_ratios': ratios,
                                   'ci95': [boots[249], boots[9749]]}
    return out


def pinned(cmd):
    return cmd if MAC else ['taskset', '-c', '2', *cmd]


def runtime():
    verify_compilers()
    assert len(json.loads((E / 'checks.json').read_text())) == 76
    build = json.loads((E / 'build.json').read_text())
    canonical = E / 'runtime/app'; canonical.parent.mkdir()
    rows = {}
    for app in APPS:
        for bench in APPS[app][1]:
            runs = []; samples = {c: [] for c in CONFIGS}
            expected = (P / app / (bench + '-clang.stdout')).read_bytes()
            for block in range(-2, 12):
                for c in order(block):
                    ident = build[c][app]['binary']; binary = Path(ident['path'])
                    assert sha(binary) == ident['sha256']
                    shutil.copy2(binary, canonical)
                    assert sha(canonical) == ident['sha256']
                    cmd, stdin = invocation(app, str(canonical), fixture(app), bench)
                    cmd = pinned(cmd)
                    start = time.perf_counter_ns()
                    p = subprocess.run(cmd, input=stdin, capture_output=True, timeout=180)
                    ms = (time.perf_counter_ns() - start) / 1e6
                    assert p.returncode == 0 and p.stdout == expected, (c, app, bench, p.stderr)
                    runs.append({'block': block, 'warmup': block < 0, 'config': c, 'elapsed_ms': ms,
                                 'argv': cmd, 'stdin_sha256': hashlib.sha256(stdin).hexdigest() if stdin else None,
                                 'binary_sha256': ident['sha256'], 'stdout_sha256': hashlib.sha256(p.stdout).hexdigest()})
                    if block >= 0: samples[c].append(ms)
            rows[app + '/' + bench] = {'runs': runs, **summarize(samples)}
            save(E / 'runtime.json', rows)
            print('timed', app, bench, {k: v['median'] for k, v in rows[app+'/'+bench]['contrasts'].items()}, flush=True)


def compile_cost():
    verify_compilers()
    canonical = E / 'compile/keel'; canonical.parent.mkdir()
    out = E / 'compile/out.s'
    rows = {}
    # Ordinary complete cc -O CLI invocation, .i -> assembly; includes startup,
    # read, C parsing/lowering, IR optimization, backend selection/regalloc and
    # assembly serialization/write. Excludes preprocessing, assembler and link.
    for app in APPS:
        sources = sorted((E / 'inputs' / app / 'pre').glob('*.i'))
        runs = []; samples = {c: [] for c in CONFIGS}
        for block in range(-2, 12):
            for c in order(block):
                binary = compiler(c)
                shutil.copy2(binary, canonical); assert sha(canonical) == sha(binary)
                total = 0
                for src in sources:
                    cmd = list(map(str, pinned([canonical, *flags(c), src, '-o', out])))
                    start = time.perf_counter_ns()
                    p = subprocess.run(cmd, capture_output=True, timeout=900)
                    ms = (time.perf_counter_ns() - start) / 1e6
                    assert p.returncode == 0, (cmd, p.stderr)
                    total += ms
                    runs.append({'block': block, 'warmup': block < 0, 'config': c,
                                 'source': str(src), 'elapsed_ms': ms, 'argv': cmd,
                                 'output_sha256': sha(out), 'stderr': p.stderr.decode(errors='replace')})
                if block >= 0: samples[c].append(total)
            save(E / 'compile-progress.json', {'app': app, 'block': block, 'runs': runs})
            print('compiled', app, block, flush=True)
        rows[app] = {'runs': runs, **summarize(samples)}
        save(E / 'compile.json', rows)


def inspect():
    build = json.loads((E / 'build.json').read_text())
    rows = {}
    for c in CONFIGS:
        rows[c] = {}
        for app, symbol in [('quickjs', 'JS_CallInternal'), ('sqlite', 'sqlite3FpDecode'), ('lua', 'lua_geti')]:
            binary = Path(build[c][app]['binary']['path'])
            dis = run(['xcrun', 'llvm-objdump', '-d', binary] if MAC else ['objdump', '-d', binary]).stdout.decode()
            dest = E / 'inspection'; dest.mkdir(exist_ok=True)
            with gzip.open(dest / (c + '-' + app + '.disasm.txt.gz'), 'wt') as f: f.write(dis)
            # Native function regions, including duplicate private instances.
            starts = list(re.finditer(r'^\s*[0-9a-fA-F]+ <([^>]+)>:\s*$', dis, re.M))
            regions = []
            for i, m in enumerate(starts):
                if m[1].lstrip('_') == symbol:
                    regions.append(dis[m.start():starts[i+1].start() if i+1<len(starts) else len(dis)])
            assert regions, (c, app, symbol)
            body = '\n'.join(regions)
            (dest / (c + '-' + symbol + '.txt')).write_text(body)
            counts = {op: len(re.findall(r'\b' + op + r'\b', body)) for op in
                      (['udiv', 'umulh', 'umull', 'madd', 'msub', 'cset', 'cmp', 'ldr', 'ldrb', 'add', 'sub'] if MAC else
                       ['div', 'divl', 'divq', 'mul', 'mull', 'mulq', 'imul', 'lea', 'leaq', 'cmp', 'setne', 'sete'])}
            rows[c][app] = {'symbol': symbol, 'instances': len(regions), 'instruction_counts_static': counts,
                            'prologue': [r.splitlines()[:28] for r in regions],
                            'evidence': str(dest / (c + '-' + symbol + '.txt'))}
    save(E / 'inspection.json', rows)


def manifest():
    save(E / 'manifest.json', {str(p.relative_to(E)): {'sha256': sha(p), 'bytes': p.stat().st_size}
         for p in sorted(E.rglob('*')) if p.is_file() and p.name != 'manifest.json'})


if __name__ == '__main__':
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('phase', choices=['prepare', 'build', 'check', 'runtime', 'compile', 'inspect', 'release'])
    ap.add_argument('--base', type=Path, default=Path('/tmp/keel-wave-validation'))
    ap.add_argument('--prior', type=Path, default=Path('/tmp/keel-current-app-profile/evidence'))
    args = ap.parse_args()
    B = args.base.resolve(); E = B / 'evidence'; P = args.prior.resolve()
    E.mkdir(parents=True, exist_ok=True)
    boundary(args.phase + '-start')
    {'prepare': prepare, 'build': build, 'check': check, 'runtime': runtime,
     'compile': compile_cost, 'inspect': inspect, 'release': lambda: None}[args.phase]()
    boundary(args.phase + '-end')
    manifest()
