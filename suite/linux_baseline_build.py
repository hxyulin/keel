#!/usr/bin/env python3
"""Linux GCC/Clang/Keel baseline: timed compilation, common links and correctness.

Runs on the benchmark host inside an isolated scratch root that already holds
an extracted source tree and the suite's preprocessed files (`cargo suite test`
creates them under `target/suite/<app>/pre`). Every compiler reads the same
clang-preprocessed `.i` bytes, except GCC: glibc headers expanded for Clang
declare `_Float128` in a way GCC rejects, so GCC compiles its own `gcc -E`
output of the same sources with the suite's -std/-D/-I flags (hashes are
recorded for both). Compilation is serial on one pinned CPU; each
configuration is timed per translation unit with warmups and repetitions, and
every repetition must reproduce the first one's output bytes. Codegen (`-S`)
and the shared assembler step are timed separately; direct-to-object runs are
timed as a separate mode. Runtime artifacts use `-S`, then one common Clang
assembler command and one common link command, so arms differ only in the
compiler that generated the assembly. See linux_baseline_runtime.py for timing.
"""
import argparse
import hashlib
import json
import os
import pathlib
import platform
import resource
import struct
import subprocess
import time

APPS = {
    'lua': ['lapi', 'lauxlib', 'lbaselib', 'lcode', 'lcorolib', 'lctype', 'ldblib', 'ldebug', 'ldo',
            'ldump', 'lfunc', 'lgc', 'linit', 'liolib', 'llex', 'lmathlib', 'lmem', 'loadlib',
            'lobject', 'lopcodes', 'loslib', 'lparser', 'lstate', 'lstring', 'lstrlib', 'ltable',
            'ltablib', 'ltm', 'lua', 'lundump', 'lutf8lib', 'lvm', 'lzio'],
    'quickjs': ['qjs', 'quickjs', 'libregexp', 'libunicode', 'cutils', 'quickjs-libc', 'libbf',
                'repl', 'qjscalc'],
    'sqlite': ['sqlite3', 'shell'],
}
# The suite manifests' preprocessing flags on Linux/x86-64 (suite/*/suite.toml).
PREPROCESS = {
    'lua': dict(dir='lua/lua-5.4.7/src', std='gnu99', defines=['LUA_USE_LINUX'], includes=[],
                sources={s: s + '.c' for s in APPS['lua']}),
    'quickjs': dict(dir='quickjs/quickjs-2024-01-13', std='gnu11',
                    defines=['_GNU_SOURCE', 'CONFIG_VERSION="2024-01-13"', 'CONFIG_BIGNUM'], includes=['.'],
                    sources={**{s: s + '.c' for s in APPS['quickjs'] if s not in ('repl', 'qjscalc')},
                             'repl': '../gen/repl.c', 'qjscalc': '../gen/qjscalc.c'}),
    'sqlite': dict(dir='sqlite/sqlite-amalgamation-3460100', std='c11',
                   defines=['SQLITE_THREADSAFE=1', '_GNU_SOURCE', 'LONGDOUBLE_TYPE=double'], includes=[],
                   sources={'sqlite3': 'sqlite3.c', 'shell': 'shell.c'}),
}
BINARY = {'lua': 'lua', 'quickjs': 'qjs', 'sqlite': 'sqlite3'}
LIBS = ['-lm', '-lpthread', '-ldl']
TARGET = 'x86_64-unknown-linux-gnu'
# Shared C codegen flags for GCC and Clang: x86-64-v2, Keel's x86 baseline ISA
# (crates/keel-codegen/src/x86_64.rs), with generic scheduling; PIE, no LTO, no
# strict-aliasing or FP contraction (Keel performs neither), no distro hardening
# defaults (Keel emits no stack protector, CET or stack-clash probes).
C_COMMON = ['-march=x86-64-v2', '-mtune=generic', '-fPIE', '-fno-strict-aliasing', '-ffp-contract=off',
            '-fno-lto', '-fno-stack-protector', '-fcf-protection=none', '-fno-stack-clash-protection', '-w']
KEEL_COMMON = ['--target', TARGET]
KEEL_O = ['-O', '--tune', 'generic', '--isel', 'all', '--regalloc', 'full']
ASSEMBLE = ['--target=' + TARGET, '-c', '-x', 'assembler']
LINK = ['--target=' + TARGET, '-fno-lto', '-Wl,--section-start=.init=0x10000']


def sha(path):
    h = hashlib.sha256()
    with open(path, 'rb') as f:
        for chunk in iter(lambda: f.read(1 << 20), b''):
            h.update(chunk)
    return h.hexdigest()


def configs(tools):
    gcc, clang, keel = tools['gcc'], tools['clang'], tools['keel']
    return {
        'gcc-O0': dict(family='gcc', S=[gcc, '-O0', *C_COMMON, '-S'], obj=[gcc, '-O0', *C_COMMON, '-c']),
        'gcc-O2': dict(family='gcc', S=[gcc, '-O2', *C_COMMON, '-S'], obj=[gcc, '-O2', *C_COMMON, '-c']),
        'clang-O0': dict(family='clang', S=[clang, '--target=' + TARGET, '-O0', *C_COMMON, '-S'],
                         obj=[clang, '--target=' + TARGET, '-O0', *C_COMMON, '-c']),
        'clang-O2': dict(family='clang', S=[clang, '--target=' + TARGET, '-O2', *C_COMMON, '-S'],
                         obj=[clang, '--target=' + TARGET, '-O2', *C_COMMON, '-c']),
        'keel': dict(family='keel', S=[keel, 'cc', *KEEL_COMMON], obj=[keel, 'cc', *KEEL_COMMON, '--emit-object']),
        'keel-O': dict(family='keel', S=[keel, 'cc', *KEEL_COMMON, *KEEL_O],
                       obj=[keel, 'cc', *KEEL_COMMON, *KEEL_O, '--emit-object']),
    }


ENV = dict(PATH='/usr/bin:/bin', LANG='C', LC_ALL='C', TZ='UTC')


def timed(argv, cwd, stdin=None, timeout=900):
    """Runs argv; returns (rc, wall_ns, stdout, stderr)."""
    start = time.perf_counter_ns()
    child = subprocess.Popen(argv, cwd=cwd, env=ENV, stdin=subprocess.PIPE if stdin is not None else subprocess.DEVNULL,
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    out, err = child.communicate(stdin, timeout=timeout)
    wall = time.perf_counter_ns() - start
    return child.returncode, wall, out, err


def rusage_delta(before, after):
    return after.ru_utime - before.ru_utime, after.ru_stime - before.ru_stime


def compile_once(argv, cwd):
    """Serial runs only: CPU time is the RUSAGE_CHILDREN delta."""
    before = resource.getrusage(resource.RUSAGE_CHILDREN)
    rc, wall, out, err = timed(argv, cwd)
    after = resource.getrusage(resource.RUSAGE_CHILDREN)
    user, system = rusage_delta(before, after)
    return dict(rc=rc, wall_ns=wall, user_s=user, sys_s=system, stderr=err.decode(errors='replace')[-4000:])


def elf_rx(path):
    data = pathlib.Path(path).read_bytes()
    h = struct.unpack_from('<16sHHIQQQIHHHHHH', data)
    segs = [struct.unpack_from('<IIQQQQQQ', data, h[5] + i * 56) for i in range(h[10])]
    rx = [s for s in segs if s[0] == 1 and s[1] & 1]
    s = rx[0]
    return dict(type=h[1], rx_segments=len(rx), rx_offset=s[2], rx_vaddr=s[3], rx_filesz=s[5], rx_memsz=s[6],
                rx_writable=bool(s[1] & 2),
                rx_sha256=hashlib.sha256(data[s[2] // 4096 * 4096:s[2] // 4096 * 4096 + (s[3] % 4096 + s[5] + 4095) // 4096 * 4096]).hexdigest())


def cmd_compile(args):
    root = pathlib.Path(args.root).resolve()
    out = pathlib.Path(args.out).resolve()
    out.mkdir(parents=True, exist_ok=False)
    tools = dict(gcc=args.gcc, clang=args.clang, keel=args.keel)
    table = configs(tools)
    selected = args.configs.split(',')
    modes = args.modes.split(',')
    os.sched_setaffinity(0, {args.cpu})
    tool_info = {}
    for name, path in tools.items():
        real = os.path.realpath(path)
        version = subprocess.run([path, '--version'], capture_output=True, text=True, env=ENV).stdout.splitlines()[:1] if name != 'keel' else []
        tool_info[name] = dict(path=path, realpath=real, sha256=sha(real), version=version)
    inputs = {}
    for app in args.apps.split(','):
        for stem in APPS[app]:
            for pre, base in [('clang', root / 'target' / 'suite' / app / 'pre'), ('gcc', pathlib.Path(args.gcc_pre) / app)]:
                p = base / (stem + '.i')
                inputs[(pre, app, stem)] = dict(path=str(p), sha256=sha(p), bytes=p.stat().st_size)
    meta = dict(kind='meta', host=platform.node(), uname=list(os.uname()), cpu=args.cpu, tools=tool_info,
                configs={k: {m: v[m] for m in ['S', 'obj']} for k, v in table.items() if k in selected},
                assemble=[args.clang, *ASSEMBLE], link=[args.clang, *LINK, '...', '-o', 'BIN', *LIBS],
                env=ENV, warmups=args.warmups, reps=args.reps, modes=modes,
                inputs={f'{pre}/{a}/{s}': v for (pre, a, s), v in inputs.items()},
                order='rep-major; configs rotated by rep; apps then TUs in link order; serial; one pinned CPU')
    raw = (out / 'compile-raw.jsonl').open('x')

    def emit(row):
        raw.write(json.dumps(row) + '\n')
        raw.flush()

    emit(meta)
    first = {}
    total_reps = args.warmups + args.reps
    for rep in range(total_reps):
        phase = 'warmup' if rep < args.warmups else 'measured'
        rotated = selected[rep % len(selected):] + selected[:rep % len(selected)]
        for config in rotated:
            for mode in modes:
                for app in args.apps.split(','):
                    dest = out / 'artifacts' / mode / config / app
                    dest.mkdir(parents=True, exist_ok=True)
                    for stem in APPS[app]:
                        pre = 'gcc' if table[config]['family'] == 'gcc' else 'clang'
                        src = inputs[(pre, app, stem)]['path']
                        final = dest / (stem + ('.s' if mode == 'S' else '.o'))
                        target = final if rep == 0 else dest / (stem + '.rep' + ('.s' if mode == 'S' else '.o'))
                        argv = [*table[config][mode], src, '-o', str(target)]
                        r = compile_once(argv, dest)
                        if r['rc'] != 0:
                            emit(dict(kind='failure', config=config, mode=mode, app=app, stem=stem, argv=argv, **r))
                            raise SystemExit(f'compile failed: {config} {mode} {app}/{stem}: {r["stderr"][-800:]}')
                        digest = sha(target)
                        if rep == 0:
                            first[(config, mode, app, stem)] = digest
                        elif digest != first[(config, mode, app, stem)]:
                            emit(dict(kind='failure', reason='nondeterministic output', config=config, mode=mode, app=app, stem=stem))
                            raise SystemExit(f'nondeterministic: {config} {mode} {app}/{stem}')
                        else:
                            target.unlink()
                        row = dict(kind='compile', rep=rep, phase=phase, config=config, mode=mode, app=app, stem=stem,
                                   argv=argv, output_sha256=digest, output_bytes=final.stat().st_size, **r)
                        row.pop('stderr')
                        row['stderr_bytes'] = len(r['stderr'])
                        emit(row)
                        if mode == 'S':
                            obj = dest / (stem + ('.o' if rep == 0 else '.rep.o'))
                            aargv = [args.clang, *ASSEMBLE, str(final), '-o', str(obj)]
                            a = compile_once(aargv, dest)
                            if a['rc'] != 0:
                                emit(dict(kind='failure', stage='assemble', config=config, app=app, stem=stem, argv=aargv, **a))
                                raise SystemExit(f'assemble failed: {config} {app}/{stem}: {a["stderr"][-800:]}')
                            od = sha(obj)
                            if rep == 0:
                                first[(config, 'asm', app, stem)] = od
                            elif od != first[(config, 'asm', app, stem)]:
                                raise SystemExit(f'nondeterministic assembly: {config} {app}/{stem}')
                            else:
                                obj.unlink()
                            a.pop('stderr')
                            emit(dict(kind='assemble', rep=rep, phase=phase, config=config, app=app, stem=stem,
                                      argv=aargv, output_sha256=od, **a))
            print(f'rep {rep} {config} done', flush=True)
    raw.close()
    print('COMPILE_COMPLETE', flush=True)


def cmd_preprocess_gcc(args):
    """`gcc -E` of the suite sources with the manifest flags, as keel-suite runs `clang -E`."""
    work = pathlib.Path(args.root).resolve() / 'target' / 'suite'
    out = pathlib.Path(args.out).resolve()
    records = []
    for app, spec in PREPROCESS.items():
        (out / app).mkdir(parents=True, exist_ok=False)
        cwd = work / spec['dir']
        for stem in APPS[app]:
            argv = [args.gcc, '-E', '-std=' + spec['std'], *['-D' + d for d in spec['defines']],
                    *['-I' + i for i in spec['includes']], spec['sources'][stem], '-o', str(out / app / (stem + '.i'))]
            p = subprocess.run(argv, cwd=cwd, env=ENV, capture_output=True)
            if p.returncode:
                raise SystemExit(f'gcc -E failed {app}/{stem}: {p.stderr.decode()[-1000:]}')
            records.append(dict(app=app, stem=stem, argv=argv, cwd=str(cwd), source_sha256=sha(cwd / spec['sources'][stem]),
                                output_sha256=sha(out / app / (stem + '.i'))))
    (out / 'preprocess.json').write_text(json.dumps(records, indent=2) + '\n')
    print('PREPROCESS_COMPLETE', len(records))


def cmd_link(args):
    """Links each runtime arm from `-S` artifacts (assembled by the shared command) twice, checks layout."""
    out = pathlib.Path(args.out).resolve()
    compile_dir = pathlib.Path(args.compile_dir).resolve()
    bins = out / 'bin'
    bins.mkdir(parents=True, exist_ok=False)
    records = []
    for config in args.configs.split(','):
        for app in args.apps.split(','):
            objs = [str(compile_dir / 'artifacts' / 'S' / config / app / (s + '.o')) for s in APPS[app]]
            for copy in ['A', 'B']:
                dest = bins / config / copy
                dest.mkdir(parents=True, exist_ok=True)
                binary = dest / BINARY[app]
                argv = [args.clang, *LINK, *objs, '-o', str(binary), *LIBS]
                p = subprocess.run(argv, capture_output=True, env=ENV)
                if p.returncode:
                    raise SystemExit(f'link failed {config} {app}: {p.stderr.decode()[-2000:]}')
                records.append(dict(config=config, app=app, copy=copy, argv=argv, sha256=sha(binary),
                                    objects={o: sha(o) for o in objs}, layout=elf_rx(binary),
                                    link_stderr=p.stderr.decode()[-2000:]))
    by = {}
    for r in records:
        by.setdefault((r['config'], r['app']), set()).add(r['sha256'])
    for key, digests in by.items():
        if len(digests) != 1:
            raise SystemExit(f'relink of {key} not byte-identical')
    for app in args.apps.split(','):
        vaddrs = {r['layout']['rx_vaddr'] // 4096 * 4096 for r in records if r['app'] == app}
        if len(vaddrs) != 1:
            raise SystemExit(f'{app}: unequal RX vaddrs {vaddrs}')
    for r in records:
        L = r['layout']
        if L['type'] != 3 or L['rx_segments'] != 1 or L['rx_writable'] or L['rx_filesz'] != L['rx_memsz']:
            raise SystemExit(f'unsupported layout {r["config"]} {r["app"]}: {L}')
    (out / 'link.json').write_text(json.dumps(records, indent=2) + '\n')
    print('LINK_COMPLETE', len(records))


def run_check(argv, cwd, stdin=None, timeout=600):
    p = subprocess.run(argv, cwd=cwd, env=dict(ENV, HOME=os.environ.get('HOME', '/tmp')),
                       input=stdin, capture_output=True, timeout=timeout)
    return p.returncode, p.stdout, p.stderr


def cmd_check(args):
    """Upstream portable tests per binary, then bench outputs equal across arms; writes expected stdout files."""
    root = pathlib.Path(args.root).resolve()
    out = pathlib.Path(args.out).resolve()
    suite = root / 'suite'
    work = root / 'target' / 'suite'
    configs_ = args.configs.split(',')
    results = []
    expected_dir = out / 'expected'
    expected_dir.mkdir(exist_ok=True)

    def binary(config, app, copy='A'):
        return str(out / 'bin' / config / copy / BINARY[app])

    tests = {
        'lua': [(['{bin}', '-e', '_U=true', 'all.lua'], work / 'lua' / 'lua-5.4.7-tests', None)],
        'quickjs': [(['{bin}', *t], work / 'quickjs' / 'quickjs-2024-01-13', None) for t in [
            ['tests/test_closure.js'], ['tests/test_language.js'], ['tests/test_builtin.js'], ['tests/test_loop.js'],
            ['tests/test_std.js'], ['tests/test_worker.js'], ['--bignum', 'tests/test_op_overloading.js'],
            ['--bignum', 'tests/test_bignum.js'], ['--qjscalc', 'tests/test_qjscalc.js']]],
        'sqlite': [(['{bin}', ':memory:'], work / 'sqlite', suite / 'sqlite' / 'queries.sql')],
    }
    benches = {
        'lua/fib': ('lua', ['{bin}', str(suite / 'lua' / 'bench.lua'), 'fib'], None),
        'lua/sort': ('lua', ['{bin}', str(suite / 'lua' / 'bench.lua'), 'sort'], None),
        'lua/string': ('lua', ['{bin}', str(suite / 'lua' / 'bench.lua'), 'string'], None),
        'lua/float': ('lua', ['{bin}', str(suite / 'lua' / 'bench.lua'), 'float'], None),
        'quickjs/fib': ('quickjs', ['{bin}', '--stack-size', '4194304', str(suite / 'quickjs' / 'bench.js'), 'fib'], None),
        'quickjs/sort': ('quickjs', ['{bin}', '--stack-size', '4194304', str(suite / 'quickjs' / 'bench.js'), 'sort'], None),
        'quickjs/string': ('quickjs', ['{bin}', '--stack-size', '4194304', str(suite / 'quickjs' / 'bench.js'), 'string'], None),
        'sqlite/queries': ('sqlite', ['{bin}', ':memory:'], suite / 'sqlite' / 'bench.sql'),
    }
    ok = True
    for config in configs_:
        for app, cases in tests.items():
            outputs = []
            for argv, cwd, stdin in cases:
                a = [binary(config, app) if x == '{bin}' else x for x in argv]
                rc, so, se = run_check(a, cwd, stdin.read_bytes() if stdin else None)
                outputs.append(hashlib.sha256(so).hexdigest())
                passed = rc == 0
                ok &= passed
                results.append(dict(kind='portable-test', config=config, app=app, argv=a, cwd=str(cwd), rc=rc,
                                    stdout_sha256=outputs[-1], stdout_tail=so.decode(errors='replace')[-300:],
                                    stderr_tail=se.decode(errors='replace')[-300:], passed=passed))
    # SQLite query answers must agree across every arm (as the suite compares against Clang).
    sq = {r['stdout_sha256'] for r in results if r['kind'] == 'portable-test' and r['app'] == 'sqlite'}
    results.append(dict(kind='sqlite-query-agreement', distinct_outputs=len(sq), passed=len(sq) == 1))
    ok &= len(sq) == 1
    workloads = {}
    for name, (app, argv, stdin) in benches.items():
        digests = {}
        for config in configs_:
            for copy in ['A', 'B']:
                a = [binary(config, app, copy) if x == '{bin}' else x for x in argv]
                rc, so, se = run_check(a, root, stdin.read_bytes() if stdin else None)
                digests[(config, copy)] = (rc, hashlib.sha256(so).hexdigest(), so, se)
        distinct = {v[1] for v in digests.values()}
        good = len(distinct) == 1 and all(v[0] == 0 and not v[3] for v in digests.values())
        ok &= good
        ref = digests[(configs_[0], 'A')][2]
        exp = expected_dir / (name.replace('/', '-') + '.stdout')
        exp.write_bytes(ref)
        workloads[name] = dict(app=app, argv=argv, stdin=str(stdin) if stdin else None, expected=str(exp),
                               expected_sha256=hashlib.sha256(ref).hexdigest(), stdout=ref.decode(errors='replace'))
        results.append(dict(kind='bench-output-agreement', workload=name, passed=good,
                            outputs={f'{c}/{k}': v[1] for (c, k), v in digests.items()},
                            stderr_empty=all(not v[3] for v in digests.values())))
    (out / 'correctness.json').write_text(json.dumps(dict(passed=ok, results=results, workloads=workloads), indent=2) + '\n')
    print('CHECK', 'PASS' if ok else 'FAIL')
    for r in results:
        if not r.get('passed', True):
            print('FAILED', json.dumps(r)[:600])
    if not ok:
        raise SystemExit(1)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    sub = p.add_subparsers(dest='cmd', required=True)
    c = sub.add_parser('compile')
    c.add_argument('--root', required=True)
    c.add_argument('--out', required=True)
    c.add_argument('--gcc', required=True)
    c.add_argument('--clang', required=True)
    c.add_argument('--keel', required=True)
    c.add_argument('--gcc-pre', required=True)
    c.add_argument('--cpu', type=int, required=True)
    c.add_argument('--warmups', type=int, default=1)
    c.add_argument('--reps', type=int, default=5)
    c.add_argument('--apps', default='lua,quickjs,sqlite')
    c.add_argument('--configs', default='gcc-O0,gcc-O2,clang-O0,clang-O2,keel,keel-O')
    c.add_argument('--modes', default='S,obj')
    g = sub.add_parser('preprocess-gcc')
    g.add_argument('--root', required=True)
    g.add_argument('--out', required=True)
    g.add_argument('--gcc', required=True)
    l = sub.add_parser('link')
    l.add_argument('--compile-dir', required=True)
    l.add_argument('--out', required=True)
    l.add_argument('--clang', required=True)
    l.add_argument('--apps', default='lua,quickjs,sqlite')
    l.add_argument('--configs', default='gcc-O2,clang-O2,keel-O')
    k = sub.add_parser('check')
    k.add_argument('--root', required=True)
    k.add_argument('--out', required=True)
    k.add_argument('--configs', default='gcc-O2,clang-O2,keel-O')
    args = p.parse_args()
    dict(compile=cmd_compile, link=cmd_link, check=cmd_check, **{'preprocess-gcc': cmd_preprocess_gcc})[args.cmd](args)


if __name__ == '__main__':
    main()
