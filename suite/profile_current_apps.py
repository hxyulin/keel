"""Current application runtime evidence; separate build/check/time/profile phases.

No compiler or application transformations. Profiles use ordinary Keel cc -O
binaries with symbols, and Clang line tables only after machine-code parity.
Retained originals are read only. See the dated report for host reservations.
"""
import argparse
import collections
import hashlib
import json
import math
import os
import pathlib
import platform
import random
import re
import shutil
import statistics
import subprocess
import time

from profile_app_loops import APPS, invocation, text_bytes
from measure_passes import summary

ROOT = pathlib.Path(__file__).resolve().parent.parent
MAC = platform.system() == 'Darwin'


def sha(p):
    return hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest()


def save(p, value):
    p.write_text(json.dumps(value, indent=2) + '\n')


def run(cmd, *, input=None, cwd=None, required=True, timeout=600):
    cmd = list(map(str, cmd))
    result = subprocess.run(cmd, input=input, cwd=cwd, capture_output=True, timeout=timeout)
    with (WORK / 'commands.jsonl').open('a') as f:
        f.write(json.dumps({'argv': cmd, 'cwd': str(cwd) if cwd else os.getcwd(),
                            'returncode': result.returncode, 'stderr': result.stderr.decode(errors='replace')}) + '\n')
    if required and result.returncode:
        raise RuntimeError(f'{cmd}: {result.returncode}\n{result.stderr.decode(errors="replace")}')
    return result


def activity(phase):
    cmd = ['ps', '-Ao', 'pid,pcpu,comm'] if MAC else ['ps', '-eo', 'pid,pcpu,comm', '--sort=-pcpu']
    (WORK / f'activity-{phase}.txt').write_bytes(run(cmd).stdout + run(['uptime']).stdout)


def prepare(args):
    old = json.loads((args.prior / 'provenance.json').read_text())
    previous = json.loads((args.prior / 'apps.json').read_text())
    expected = old['compilers']['after']
    assert args.compiler.resolve() == pathlib.Path(expected['binary']).resolve()
    assert sha(args.compiler) == expected['sha256']
    assert run(['git', '-C', expected['checkout'], 'rev-parse', 'HEAD']).stdout.decode().strip() == expected['revision']
    assert not run(['git', '-C', expected['checkout'], 'status', '--porcelain']).stdout.strip()
    tree = run(['git', '-C', expected['checkout'], 'rev-parse', 'HEAD:crates']).stdout.decode().strip()
    assert tree == expected['crates_tree']
    inputs = {}
    for app, count in [('lua', 33), ('quickjs', 9), ('sqlite', 2)]:
        sources = sorted((args.prepared / app / 'pre').glob('*.i'))
        assert len(sources) == count, (app, len(sources))
        dest = WORK / 'inputs' / app / 'pre'; dest.mkdir(parents=True, exist_ok=True)
        for src in sources:
            assert sha(src) == previous['sources_sha256'][str(src)], src
            shutil.copy2(src, dest / src.name)
            inputs[str(dest / src.name)] = {'original': str(src), 'sha256': sha(src)}
    fixtures = {}
    for app in APPS:
        dest = WORK / 'fixtures' / app; dest.mkdir(parents=True, exist_ok=True)
        for src in (ROOT / 'suite' / app).iterdir():
            if src.suffix not in ['.lua', '.js', '.sql', '.toml']:
                continue
            matches = [v for p, v in old['datasets'].items() if p.endswith('/suite/' + app + '/' + src.name)]
            assert len(matches) == 1 and sha(src) == matches[0], src
            shutil.copy2(src, dest / src.name); fixtures[str(dest / src.name)] = sha(src)
    tests = WORK / 'lua-tests'
    if not tests.exists():
        shutil.copytree(args.prepared / 'lua/lua-5.4.7-tests', tests)
    quicktests = WORK / 'quickjs-tests'
    if not quicktests.exists():
        shutil.copytree(args.prepared / 'quickjs/quickjs-2024-01-13', quicktests,
                        ignore=shutil.ignore_patterns('*.o', 'qjs', 'qjsc', '.git'))
    drivers = {}
    for name, flags in [('stock', ['-O2']), ('contract_off', ['-O2', '-ffp-contract=off'])]:
        raw = run(['clang', *flags, '-###', '-x', 'c', '-c', '/dev/null']).stderr.decode()
        (WORK / f'clang-{name}-driver.txt').write_text(raw)
        drivers[name] = {'flags': flags, 'raw': raw,
                         'loop_vectorizer': '"-vectorize-loops"' in raw,
                         'slp_vectorizer': '"-vectorize-slp"' in raw}
    result = {'compiler': expected, 'crates_tree': tree, 'inputs': inputs, 'fixtures': fixtures,
              'test_inputs': {str(p.relative_to(WORK)): sha(p) for base in [tests, quicktests]
                              for p in sorted(base.rglob('*')) if p.is_file()},
              'host': platform.platform(), 'clang': run(['clang', '--version']).stdout.decode(),
              'environment': dict(os.environ), 'drivers': drivers,
              'harness_sha256': sha(__file__),
              'dependencies_sha256': {n: sha(ROOT / 'suite' / n) for n in ['profile_app_loops.py', 'measure_passes.py']},
              'cpu_affinity': 2 if not MAC else None,
              'method': 'Stock Clang O2 primary; no native CPU, LTO, PGO, fast-math or Keel AVX opt-in. Keel Full default features. Exact ordinary binaries profiled. Timing and profiles separate. Same canonical runtime path for both timed configurations; copy before timer. Linux CPU2; macOS default scheduler.'}
    save(WORK / 'provenance.json', result)


def fixture(app):
    return WORK / 'fixtures' / app / ('bench.lua' if app == 'lua' else 'bench.js' if app == 'quickjs' else 'bench.sql')


def build(args):
    report = {'apps': {}, 'keel_args': ['cc', '-O'], 'clang_flags': ['-O2']}
    for app in APPS:
        directory = WORK / app; directory.mkdir(exist_ok=True)
        units = []
        for src in sorted((WORK / 'inputs' / app / 'pre').glob('*.i')):
            stem = directory / src.stem
            asm = stem.with_suffix('.s'); obj = stem.with_suffix('.o')
            run([args.compiler, 'cc', '-O', src, '-o', asm])
            run(['clang', '-c', asm, '-o', obj])
            objects = {'keel': str(obj)}
            for variant, flags in [('clang', ['-O2']), ('clang_line', ['-O2', '-gline-tables-only']),
                                   ('clang_contract_off', ['-O2', '-ffp-contract=off'])]:
                out = directory / f'{src.stem}.{variant}.o'
                remarks = run(['clang', *flags, '-Rpass=(loop-vectorize|slp-vectorizer)', '-c', src, '-o', out])
                (directory / f'{src.stem}.{variant}.remarks.txt').write_bytes(remarks.stderr)
                objects[variant] = str(out)
            assert text_bytes(objects['clang']) == text_bytes(objects['clang_line']), src
            units.append({'source': str(src), 'sha256': sha(src), 'objects': objects,
                          'clang_line_text_parity': True,
                          'text_hashes': {v: hashlib.sha256(text_bytes(p)).hexdigest() for v, p in objects.items()}})
            print('built', app, src.name, flush=True)
        binaries = {}
        for variant in ['keel', 'clang', 'clang_line', 'clang_contract_off']:
            binary = directory / f'{APPS[app][0]}-{variant}'
            run(['clang', *[u['objects'][variant] for u in units], '-o', binary,
                 '-lm', '-lpthread', *(['-ldl'] if not MAC else [])])
            binaries[variant] = str(binary)
        assert text_bytes(binaries['clang']) == text_bytes(binaries['clang_line'])
        details = {'units': units, 'binaries': binaries,
                   'binary_sha256': {v: sha(p) for v, p in binaries.items()},
                   'text_bytes': {v: len(text_bytes(p)) for v, p in binaries.items()},
                   'file_bytes': {v: pathlib.Path(p).stat().st_size for v, p in binaries.items()},
                   'clang_linked_line_text_parity': True,
                   'clang_contract_off_text_identical': text_bytes(binaries['clang']) == text_bytes(binaries['clang_contract_off'])}
        for v, p in binaries.items():
            (directory / f'{v}.nm.txt').write_bytes(run(['nm', '-n', p]).stdout)
            dis = ['xcrun', 'llvm-objdump', '-d', p] if MAC else ['objdump', '-d', p]
            (directory / f'{v}.disasm.txt').write_bytes(run(dis).stdout)
        report['apps'][app] = details
        save(WORK / 'build.json', report)


def check(report):
    checks = []
    for app, detail in report['apps'].items():
        for bench in APPS[app][1]:
            outputs = {}
            for variant, binary in detail['binaries'].items():
                cmd, stdin = invocation(app, binary, fixture(app), bench)
                res = run(cmd, input=stdin)
                path = WORK / app / f'{bench}-{variant}.stdout'; path.write_bytes(res.stdout)
                outputs[variant] = res.stdout
            assert len(set(outputs.values())) == 1, (app, bench)
            checks.append({'app': app, 'workload': bench, 'stdout': outputs['clang'].decode(), 'passed': True})
        for variant in ['keel', 'clang', 'clang_contract_off']:
            binary = detail['binaries'][variant]
            if app == 'lua':
                dest = WORK / f'lua-tests-{variant}'
                if not dest.exists(): shutil.copytree(WORK / 'lua-tests', dest)
                res = run([binary, '-e', '_U=true', 'all.lua'], cwd=dest, required=False)
                (WORK / app / f'tests-{variant}.log').write_bytes(res.stdout + res.stderr)
                checks.append({'app': app, 'variant': variant, 'test': 'portable', 'exit': res.returncode, 'passed': res.returncode == 0})
            elif app == 'sqlite':
                res = run([binary, ':memory:'], input=(WORK / 'fixtures/sqlite/queries.sql').read_bytes())
                (WORK / app / f'tests-{variant}.log').write_bytes(res.stdout)
            else:
                for test in ['test_closure', 'test_language', 'test_builtin', 'test_loop', 'test_std', 'test_worker',
                             'test_op_overloading', 'test_bignum', 'test_qjscalc']:
                    flags = ['--qjscalc'] if test == 'test_qjscalc' else ['--bignum'] if test in ['test_op_overloading', 'test_bignum'] else []
                    res = run([binary, *flags, 'tests/' + test + '.js'], cwd=WORK / 'quickjs-tests', required=False)
                    (WORK / app / f'{test}-{variant}.log').write_bytes(res.stdout + res.stderr)
                    checks.append({'app': app, 'variant': variant, 'test': test, 'exit': res.returncode, 'passed': res.returncode == 0})
        if app == 'sqlite':
            assert len({(WORK / app / f'tests-{v}.log').read_bytes() for v in ['keel', 'clang', 'clang_contract_off']}) == 1
            checks.append({'app': app, 'test': 'queries.sql', 'passed': True})
        save(WORK / 'checks.json', checks)


def timing(report):
    rows = {}
    canonical = WORK / 'runtime' / 'app'; canonical.parent.mkdir(exist_ok=True)
    for app, details in report['apps'].items():
        for bench in APPS[app][1]:
            samples = {'keel': [], 'clang': []}; runs = []
            reference = (WORK / app / f'{bench}-clang.stdout').read_bytes()
            for iteration in range(13):
                order = ['clang', 'keel'] if iteration % 2 == 0 else ['keel', 'clang']
                for variant in order:
                    binary = details['binaries'][variant]
                    assert sha(binary) == details['binary_sha256'][variant]
                    shutil.copy2(binary, canonical)
                    cmd, stdin = invocation(app, str(canonical), fixture(app), bench)
                    if not MAC: cmd = ['taskset', '-c', '2', *cmd]
                    start = time.perf_counter_ns()
                    res = subprocess.run(cmd, input=stdin, capture_output=True, timeout=180)
                    elapsed = (time.perf_counter_ns() - start) / 1e6
                    assert res.returncode == 0, res.stderr
                    assert res.stdout == reference, (app, bench, variant)
                    runs.append({'iteration': iteration, 'warmup': iteration < 2, 'variant': variant,
                                 'argv': cmd, 'elapsed_ms': elapsed, 'binary_sha256': sha(canonical)})
                    if iteration >= 2: samples[variant].append(elapsed)
            rows[app + '/' + bench] = {'runs': runs, **summary({'before': samples['clang'], 'after': samples['keel']}, random.Random(42))}
            save(WORK / 'timing.json', rows)
            print('timed', app, bench, rows[app + '/' + bench]['after_over_before'], flush=True)


def mac_leaves(path):
    raw = path.read_text(); tree = raw.split('Call graph:', 1)[1].split('Total number in stack', 1)[0]
    # A tree node's self count is its inclusive count minus its immediate children.
    nodes = []; stack = []; leaves = collections.Counter(); inclusive = collections.Counter()
    for line in tree.splitlines():
        m = re.match(r'^(\s*[+!:| ]*?)(\d+) (.+)$', line)
        if not m: continue
        indent = len(m[1]); count = int(m[2]); name = m[3]
        while stack and nodes[stack[-1]]['indent'] >= indent: stack.pop()
        node = {'indent': indent, 'count': count, 'children': 0, 'name': name}
        if stack: nodes[stack[-1]]['children'] += count
        nodes.append(node); stack.append(len(nodes) - 1)
    roots = [n for n in nodes if 'Thread_' in n['name']]
    total = sum(n['count'] for n in roots)
    for node in nodes:
        if 'Thread_' in node['name']: continue
        own = node['count'] - node['children']
        assert own >= 0, node
        leaves[node['name']] += own
        inclusive[node['name']] += node['count']
    assert sum(leaves.values()) == total, (sum(leaves.values()), total)
    # Recursive inclusive nodes may count one sample more than once. Do not sum them.
    return {'denominator': total, 'denominator_unit': 'wall thread samples',
            'leaf_rows': [{'location': k, 'samples': v, 'percent': 100*v/total} for k, v in leaves.most_common() if v],
            'inclusive_node_counts_not_deduplicated': dict(inclusive),
            'unattributed_leaf_samples': sum(v for k, v in leaves.items() if '???' in k or 'unknown' in k.lower()),
            'unwind_note': 'sample native call tree retained; recursion inclusive counts are not unique samples. Missing caller depth cannot be quantified; no reconstructed callchains.'}


def perf_leaves(raw):
    rows = collections.Counter(); records = 0
    for line in raw.splitlines():
        m = re.match(r'\s*(\d+)\s+([0-9a-f]+)\s+(.+?)\s+\((.+?)\)\s*$', line)
        if m:
            period, ip, symbol, dso = m.groups(); rows[(ip, symbol, dso)] += int(period); records += 1
    total = sum(rows.values()); assert total
    return {'denominator': total, 'denominator_unit': 'sample period weighted user cycles', 'sample_records': records,
            'leaf_rows': [{'ip': ip, 'location': sym, 'image': dso, 'weight': v, 'percent': 100*v/total}
                          for (ip, sym, dso), v in rows.most_common()],
            'unattributed_weight': sum(v for (_, sym, dso), v in rows.items() if sym == '[unknown]' or dso == '[unknown]'),
            'unwind_note': 'IP-only native sampling, no callchain requested: caller coverage 0%; inclusive cost unavailable. No false reconstructed callchains.'}


def profiles(args, report):
    rows = {}
    for app, detail in report['apps'].items():
        for bench in APPS[app][1]:
            reference = (WORK / app / f'{bench}-clang.stdout').read_bytes(); captures = []
            for variant in ['keel', 'clang']:
                binary = detail['binaries']['clang_line' if variant == 'clang' else 'keel']
                assert sha(binary) == detail['binary_sha256']['clang_line' if variant == 'clang' else 'keel']
                cmd, stdin = invocation(app, binary, fixture(app), bench)
                start = time.perf_counter(); res = run(cmd, input=stdin); elapsed = time.perf_counter() - start
                assert res.stdout == reference
                repeats = max(2, math.ceil(9 / elapsed))
                original = fixture(app); repeated = WORK / app / f'{bench}-{variant}-repeat{original.suffix}'
                source = original.read_text()
                if app == 'lua': source = f'for __profile_repeat=1,{repeats} do\n{source}\nend\n'
                elif app == 'quickjs': source = f'for(let __profile_repeat=0;__profile_repeat<{repeats};__profile_repeat++){{\n{source}\n}}\n'
                else: source = ('DROP TABLE IF EXISTS t;\n' + source + '\n') * repeats
                repeated.write_text(source)
                cmd, stdin = invocation(app, binary, repeated, bench)
                for round_index in range(2):
                    prefix = WORK / app / f'{bench}-{variant}-{round_index}'
                    start = time.perf_counter()
                    if MAC:
                        out = prefix.with_suffix('.stdout')
                        with out.open('wb') as stream:
                            proc = subprocess.Popen(cmd, stdin=repeated.open('rb') if stdin else None, stdout=stream, stderr=subprocess.PIPE)
                            try:
                                time.sleep(.15)
                                sampler = ['sample', str(proc.pid), '6', '1', '-fullPaths', '-file', prefix.with_suffix('.sample.txt')]
                                run(sampler)
                                _, error = proc.communicate(timeout=180)
                                assert proc.returncode == 0, error
                            finally:
                                if proc.poll() is None: proc.kill(); proc.wait()
                        assert out.read_bytes() == reference * repeats
                        result = mac_leaves(prefix.with_suffix('.sample.txt'))
                    else:
                        perf = str(args.perf)
                        sampler = ['sudo', '-n', 'env', 'LD_LIBRARY_PATH=' + str(args.perf.parents[1] / 'lib'), perf,
                                   'record', '-e', 'cycles:u', '-F', '499', '--sample-cpu', '-o', prefix.with_suffix('.perf.data'),
                                   '--', 'taskset', '-c', '2', *cmd]
                        res = run(sampler, input=stdin)
                        prefix.with_suffix('.record.log').write_bytes(res.stderr)
                        prefix.with_suffix('.stdout').write_bytes(res.stdout)
                        assert res.stdout == reference * repeats
                        base = ['sudo', '-n', 'env', 'LD_LIBRARY_PATH=' + str(args.perf.parents[1] / 'lib'), perf]
                        raw = run([*base, 'script', '-i', prefix.with_suffix('.perf.data'), '-F', 'period,ip,sym,symoff,dso']).stdout
                        prefix.with_suffix('.perf.txt').write_bytes(raw)
                        prefix.with_suffix('.perf-header.txt').write_bytes(run([*base, 'report', '--header-only', '-i', prefix.with_suffix('.perf.data')]).stdout)
                        prefix.with_suffix('.perf-events.txt').write_bytes(run([*base, 'script', '-D', '-i', prefix.with_suffix('.perf.data')]).stdout)
                        result = perf_leaves(raw.decode())
                    captures.append({'variant': variant, 'round': round_index, 'repetitions': repeats,
                                     'fixture_sha256': sha(repeated), 'argv': cmd, 'sampler': list(map(str, sampler)),
                                     'elapsed_capture_and_workload_s': time.perf_counter() - start,
                                     'raw_prefix': str(prefix), **result})
                    rows[app + '/' + bench] = captures; save(WORK / 'profiles.json', rows)
                    print('profiled', app, bench, variant, round_index, flush=True)


def inspect(args, report):
    for app, detail in report['apps'].items():
        # Print matching optimized IR for code inspection without changing measured binaries.
        stems = {'lua': ['lvm', 'ldo', 'ltable', 'lstrlib', 'lmathlib', 'ltablib'],
                 'quickjs': ['quickjs', 'libregexp', 'libunicode'], 'sqlite': ['sqlite3']}[app]
        for unit in detail['units']:
            src = pathlib.Path(unit['source'])
            if src.stem not in stems: continue
            (WORK / app / (src.stem + '.keel')).write_bytes(run([args.compiler, 'cc', '-O', '--emit-ir', src]).stdout)
            run(['clang', '-O2', '-S', '-emit-llvm', src, '-o', WORK / app / (src.stem + '.ll')])


def main():
    global WORK
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('phase', choices=['prepare', 'build', 'check', 'time', 'profile', 'inspect'])
    p.add_argument('--compiler', type=pathlib.Path, default=pathlib.Path('/tmp/keel-bench-target-6e6d2c2/release/keel'))
    p.add_argument('--prepared', type=pathlib.Path, required=True)
    p.add_argument('--prior', type=pathlib.Path, required=True)
    p.add_argument('--work', type=pathlib.Path, required=True)
    p.add_argument('--perf', type=pathlib.Path)
    args = p.parse_args(); WORK = args.work.resolve(); WORK.mkdir(parents=True, exist_ok=True)
    activity(args.phase)
    if args.phase == 'prepare': prepare(args); return
    provenance = json.loads((WORK / 'provenance.json').read_text())
    assert sha(args.compiler) == provenance['compiler']['sha256']
    assert sha(__file__) == provenance['harness_sha256']
    for path, record in provenance['inputs'].items(): assert sha(path) == record['sha256']
    if args.phase == 'build': build(args); return
    report = json.loads((WORK / 'build.json').read_text())
    for detail in report['apps'].values():
        for variant, path in detail['binaries'].items(): assert sha(path) == detail['binary_sha256'][variant]
    if args.phase == 'check': check(report)
    elif args.phase == 'time': timing(report)
    elif args.phase == 'profile': profiles(args, report)
    elif args.phase == 'inspect': inspect(args, report)


if __name__ == '__main__': main()
