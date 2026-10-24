#!/usr/bin/env python3
"""Download-free SIMD correctness/codegen comparison; measurements are opt-in."""
import argparse
import collections
import json
import math
import os
from pathlib import Path
import platform
import random
import re
import shutil
import struct
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
sys.path.insert(0, str(HERE.parent))
from profile_app_loops import sha, text_bytes

CASES = ('add', 'multiply', 'compare', 'widen', 'shuffle', 'reduce', 'copy', 'float-add',
         'permute-dword', 'shuffle-pairs', 'compare-u8')
PROFILES = {
    'v1': ('x86_64-unknown-linux-gnu', 'x86-64'),
    'v2': ('x86_64-unknown-linux-gnu', 'x86-64-v2'),
    'v3': ('x86_64-unknown-linux-gnu', 'x86-64-v3'),
    'apple-arm64': ('aarch64-apple-darwin', 'armv8-a'),
}
VARIANTS = {
    'none': ['--opt-tier', 'none', '--isel-tier', 'baseline', '--regalloc', 'full'],
    'fast': ['--opt-tier', 'fast', '--isel-tier', 'full', '--regalloc', 'full'],
    'full': ['--opt-tier', 'full', '--isel-tier', 'full', '--regalloc', 'full'],
    'full-baseline-select': ['--opt-tier', 'full', '--isel-tier', 'baseline', '--regalloc', 'full'],
    'full-fast-alloc': ['--opt-tier', 'full', '--isel-tier', 'full', '--regalloc', 'fast'],
}


def native_allowed(profile, system, machine, flags, translated=False):
    """Fail closed. Linux's AVX flag includes OSXSAVE/XCR0 state enablement.

    Linux does not expose a separate `osxsave` cpuinfo flag. Requiring `avx`
    (not just the raw CPUID AVX2 bit) relies on the kernel's OS-state check.
    """
    if profile not in PROFILES or translated:
        return False
    if profile == 'apple-arm64':
        return system == 'Darwin' and machine in ('arm64', 'aarch64')
    if system != 'Linux' or machine != 'x86_64':
        return False
    required = {'sse2'}
    if profile in ('v2', 'v3'):
        required |= {'pni', 'ssse3', 'sse4_1', 'sse4_2', 'popcnt', 'cx16', 'lahf_lm'}
    if profile == 'v3':
        required |= {'avx', 'avx2', 'bmi1', 'bmi2', 'f16c', 'fma', 'abm', 'movbe', 'xsave'}
    return required <= set(flags)


def corpus(case):
    """Independent scalar, modular/bit-exact oracle; same data for every variant."""
    rng = random.Random(20261023)
    edges = [0, 1, 0xffffffff, 0x80000000, 0x7fffffff, 0x55555555, 0xaaaaaaaa]
    rows = []
    signed = lambda x: x if x < 1 << 31 else x - (1 << 32)
    for i in range(64):
        a = [edges[(i+j) % len(edges)] if i < 7 else rng.getrandbits(32) for j in range(4)]
        b = [edges[(i+2*j+1) % len(edges)] if i < 7 else rng.getrandbits(32) for j in range(4)]
        if case == 'compare-u8':
            # Rotate boundary pairs through every lane before seeded random rows.
            pairs = [(0, 255), (255, 0), (127, 128), (128, 127),
                     (0, 0), (127, 127), (128, 128), (255, 255)]
            ab = [pairs[(i+j) % len(pairs)] if i < len(pairs)
                  else (rng.randrange(256), rng.randrange(256)) for j in range(16)]
            aa, bb = bytes(x for x, _ in ab), bytes(y for _, y in ab)
            rows.append(aa + bb + bytes(255 if x < y else 0 for x, y in ab))
            continue
        if case == 'float-add':
            # Exactly representable finite sums; no FP-environment/NaN claims.
            a = [rng.randrange(-1000, 1001) for _ in range(4)]
            b = [rng.randrange(-1000, 1001) for _ in range(4)]
            rows.append(b''.join(struct.pack('<4f', *v) for v in (a, b, [x+y for x,y in zip(a,b)])))
            continue
        if case == 'add': out = [(x+y) & 0xffffffff for x,y in zip(a,b)]
        elif case == 'multiply': out = [(x*y) & 0xffffffff for x,y in zip(a,b)]
        elif case == 'compare': out = [0xffffffff if signed(x) < signed(y) else 0 for x,y in zip(a,b)]
        elif case == 'permute-dword': out = [a[2], a[0], a[3], a[1]]
        elif case == 'shuffle-pairs': out = [a[1], a[0], b[3], b[2]]
        elif case == 'shuffle': out = [a[3], b[0], a[1], b[2]]
        elif case == 'reduce': out = [sum(a) & 0xffffffff]*4
        elif case == 'copy': out = a
        elif case == 'widen':
            rows.append(struct.pack('<8I2q', *a, *b, *map(signed, a[:2])))
            continue
        else: raise ValueError(case)
        rows.append(struct.pack('<12I', *a, *b, *out))
    return b''.join(rows)


def probe_matches(route, profile, result):
    if profile == 'v3':
        return result['returncode'] == 0
    diagnostic = ('32-byte C vectors require the x86-64 target and AVX/AVX2 code generation'
                  if route == 'c' and profile == 'apple-arm64'
                  else 'requires a backend with 256-bit SIMD value support')
    return (result['returncode'] is not None and result['returncode'] > 0
            and diagnostic in result['stderr'])


def inspect_assembly(source):
    ops = collections.Counter()
    stack = []
    for line in source.splitlines():
        line = line.strip()
        if not line or line.startswith(('.', '#', '//', ';')) or ':' in line:
            continue
        op = line.split()[0]
        if re.fullmatch(r'[a-z][a-z0-9.]*', op):
            ops[op] += 1
            if re.search(r'\b(?:rsp|rbp|sp|x29)\b', line): stack.append(line)
    return {'assembly_opcode_counts': dict(ops),
            'stack_reference_candidates': stack,
            'confirmed_spills': None,
            'spill_note': 'Stack references include frames/saves/legalization; not allocator spill counts.'}


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--keel', type=Path, required=True)
    p.add_argument('--baseline-keel', type=Path, help='frozen revision; paired Full comparison')
    p.add_argument('--clang', default='clang')
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--profiles', nargs='+', choices=PROFILES, default=list(PROFILES))
    p.add_argument('--variants', nargs='+', choices=VARIANTS, default=['none', 'fast', 'full'])
    p.add_argument('--cases', nargs='+', choices=CASES, default=list(CASES))
    p.add_argument('--benchmark', action='store_true')
    p.add_argument('--rounds', type=int, default=7)
    p.add_argument('--repeats', type=int, default=100000)
    p.add_argument('--seed', type=int, default=1761)
    p.add_argument('--cpu', type=int, help='Linux affinity; inherited by children')
    p.add_argument('--objdump', help='optional LLVM objdump for object disassembly')
    a = p.parse_args()
    a.output, a.keel = a.output.resolve(), a.keel.resolve()
    if a.baseline_keel:
        a.baseline_keel = a.baseline_keel.resolve()
        if not a.baseline_keel.is_file(): p.error('existing baseline compiler required')
        if a.variants != ['full']: p.error('baseline comparison requires --variants full')
        if a.benchmark and a.rounds < 7: p.error('paired comparison requires at least seven rounds')
        if a.benchmark and platform.system() == 'Linux' and a.cpu is None:
            p.error('Linux paired timing requires --cpu affinity')
    if a.rounds < 1 or not 1 <= a.repeats <= 100000000:
        p.error('positive rounds and repeats in [1,100000000] required')
    output_roots = [(ROOT / base).resolve() for base in ('target', 'suite/reports')]
    output_roots.append(Path('/tmp').resolve())
    if not any(a.output.is_relative_to(base) for base in output_roots):
        p.error('output must be under /tmp, this checkout target/ or suite/reports/')
    if a.output.exists(): p.error('use a fresh output directory')
    clang = shutil.which(a.clang)
    if not clang or not a.keel.is_file(): p.error('existing Keel and Clang binaries required')
    if a.cpu is not None:
        if not hasattr(os, 'sched_setaffinity'): p.error('--cpu requires Linux')
        os.sched_setaffinity(0, {a.cpu})
    a.output.mkdir(parents=True)
    # Reuse suite-wide lock; public entry never nests another wrapper.
    if os.environ.get('KEEL_SIMD_LOCKED') != str(a.output):
        env = dict(os.environ, KEEL_SIMD_LOCKED=str(a.output))
        # The worker must see a new directory; the lock log is written on completion.
        a.output.rmdir()
        return subprocess.call([sys.executable, HERE.parent/'fast_opt_locked.py',
                                a.output/'lock.jsonl', sys.executable, '-B', __file__, *sys.argv[1:]], env=env)
    report = {'schema': 1, 'complete': False, 'benchmark': a.benchmark,
              'seed': a.seed, 'input_seed': 20261023, 'started_unix': time.time(),
              'rounds': a.rounds, 'repeats': a.repeats,
              'host': platform.platform(), 'machine': platform.machine(),
              'conditions': {'load_before': os.getloadavg(),
                             'affinity': sorted(os.sched_getaffinity(0)) if hasattr(os, 'sched_getaffinity') else None,
                             'timer': 'CLOCK_MONOTONIC; external calls + checksum/sink; warm repeated inputs',
                             'compile_boundary': 'CLI to native object; startup/I/O included, link excluded',
                             'modeled_estimates': None,
                             'compile_warmups': 1 if a.benchmark else 0,
                             'runtime_warmups': 1 if a.benchmark else 0,
                             'lto': False, 'fp_contract': 'off',
                             'runtime_repetitions': a.repeats if a.benchmark else None},
              'identities': {}, 'commands': [], 'rows': [], 'capability_probes': []}
    def save():
        (a.output/'report.json').write_text(json.dumps(report, indent=2)+'\n')
    def run(argv, data=None, optional=False):
        start = time.perf_counter()
        try:
            r = subprocess.run(list(map(str, argv)), input=data, capture_output=True, timeout=120)
            record = {'argv': list(map(str, argv)), 'seconds': time.perf_counter()-start,
                      'returncode': r.returncode, 'stdout': r.stdout.decode(errors='replace'),
                      'stderr': r.stderr.decode(errors='replace')}
        except subprocess.TimeoutExpired as exc:
            record = {'argv': list(map(str, argv)), 'seconds': time.perf_counter()-start,
                      'returncode': None, 'stderr': str(exc), 'stdout': ''}
        report['commands'].append(record)
        if record['returncode'] != 0 and not optional:
            raise RuntimeError(f"command failed: {record}")
        return record
    def freeze(path): report['identities'][str(path)] = sha(path)
    def verify():
        for path, digest in report['identities'].items():
            if sha(path) != digest: raise RuntimeError(f'identity changed: {path}')
    def preflight():
        # A shared lock excludes cooperating builds, not unrelated host work.
        load = os.getloadavg()
        busy = {}
        if a.cpu is not None:
            siblings = Path(f'/sys/devices/system/cpu/cpu{a.cpu}/topology/thread_siblings_list').read_text().strip()
            cpus = set()
            for part in siblings.split(','):
                ends = list(map(int, part.split('-')))
                cpus.update(range(ends[0], ends[-1]+1))
            def ticks():
                return {int(s.split()[0][3:]): list(map(int, s.split()[1:]))
                        for s in Path('/proc/stat').read_text().splitlines()
                        if s.split()[0] in {f'cpu{c}' for c in cpus}}
            before = ticks(); time.sleep(.25); after = ticks()
            for cpu in cpus:
                delta = [y-x for x,y in zip(before[cpu], after[cpu])]
                busy[cpu] = 1-(delta[3]+delta[4])/max(1, sum(delta[:8]))
        report.setdefault('preflight', []).append(dict(unix=time.time(), load=load, cpu_busy=busy))
        save()
        if load[0] > 1 or any(v > .1 for v in busy.values()):
            raise RuntimeError('contended host; timing deferred')
    save()
    failed = False
    try:
        for path in [*HERE.glob('*.py'), *HERE.glob('*.c'), *HERE.glob('*.keel'), Path(clang).resolve(), a.keel,
                     HERE.parent/'profile_app_loops.py', HERE.parent/'fast_opt_locked.py']:
            freeze(path)
        if a.baseline_keel: freeze(a.baseline_keel)
        revision = run(['git', '-C', ROOT, 'rev-parse', 'HEAD'], optional=True)
        report['revision'] = revision['stdout'].strip() if revision['returncode'] == 0 else None
        status = run(['git', '-C', ROOT, 'status', '--porcelain'], optional=True)
        report['dirty'] = status['stdout'] if status['returncode'] == 0 else None
        report['clang_version'] = run([clang, '--version'])['stdout']
        if platform.system() == 'Darwin':
            cpu = run(['sysctl', '-a'], optional=True)['stdout']
            report['cpu'] = '\n'.join(s for s in cpu.splitlines() if s.startswith(('machdep.cpu.', 'hw.model:', 'hw.ncpu:', 'hw.memsize:', 'sysctl.proc_translated:')))
            translated = 'sysctl.proc_translated: 1' in cpu
            flags = []
        else:
            cpu = Path('/proc/cpuinfo').read_text() if Path('/proc/cpuinfo').exists() else ''
            report['cpu'] = cpu
            flag_sets = [set(s.split(':', 1)[1].split()) for s in cpu.splitlines() if s.startswith('flags')]
            flags = sorted(set.intersection(*flag_sets)) if flag_sets else []
            translated = False
        report['detected_features'] = flags
        report['keel_identity'] = {'path': str(a.keel), 'sha256': sha(a.keel),
                                   'build_features': 'not introspectable; retain build command with frozen compiler'}
        report['conditions']['frequency_policy'] = {str(path): path.read_text().strip()
            for path in Path('/sys/devices/system/cpu').glob('cpu[0-9]*/cpufreq/scaling_governor')}
        report['conditions']['environment'] = {k: os.environ.get(k) for k in ('OMP_NUM_THREADS',)}
        jobs = []
        for profile in a.profiles:
            target, march = PROFILES[profile]
            native = native_allowed(profile, platform.system(), platform.machine(), flags, translated)
            ktarget = ['--target', target] + ([] if profile == 'apple-arm64' else ['--cpu-level', profile])
            ctarget = ['-target', target, '-march='+march, '-ffp-contract=off']
            driver = a.output/(profile+'-driver.o')
            if native: run([clang, *ctarget, '-O2', '-c', HERE/'driver.c', '-o', driver]); freeze(driver)
            # Separate the deliberately bounded C surface from backend ISA gates.
            for route in ('c', 'ir'):
                probe = a.output/(profile+'-wide-'+route+'.o')
                expected = profile == 'v3'
                result = run([a.keel, 'cc' if route == 'c' else 'compile', *ktarget,
                              '--emit-object', HERE/('wide.c' if route == 'c' else 'wide.keel'),
                              '-o', probe], optional=True)
                accepted = result['returncode'] == 0
                report['capability_probes'].append({
                    'profile': profile, 'route': route, 'accepted': accepted, 'expected': expected,
                    'limitation': 'C width / AVX value and ABI support' if route == 'c' else 'backend 256-bit ISA support',
                    'diagnostic': result['stderr'], 'passed': probe_matches(route, profile, result)})
                failed |= not probe_matches(route, profile, result)
            for case in a.cases:
                data = corpus(case)
                input_path = a.output/(case+'.inputs')
                input_path.write_bytes(data)
                freeze(input_path)
                for route in ('ir', 'c'):
                    configs = [(v, str(a.keel), VARIANTS[v]) for v in a.variants]
                    if a.baseline_keel:
                        configs.append(('baseline-full', str(a.baseline_keel), VARIANTS['full']))
                    if route == 'c': configs.append(('clang-O2', clang, ['-O2']))
                    for variant, compiler, opts in configs:
                        stem = a.output/f'{profile}-{case}-{route}-{variant}'
                        src = HERE/(case+('.keel' if route == 'ir' else '.c'))
                        obj, asm = stem.with_suffix('.o'), stem.with_suffix('.s')
                        row = {'profile': profile, 'target': target, 'case': case, 'route': route,
                               'variant': variant, 'flags': opts, 'native_allowed': native,
                               'correctness': 'not run: compilation or linking did not complete' if native else 'skipped: foreign target or unproven CPU/OS features',
                               'frontend': 'not applicable' if route == 'ir' else 'not run',
                               'backend': 'not run', 'compile_seconds': [], 'runtime_seconds': []}
                        report['rows'].append(row)
                        if variant == 'clang-O2':
                            base = [compiler, *ctarget, *opts]
                            objcmd = [*base, '-c', src, '-o', obj]
                            asmcmd = [*base, '-S', src, '-o', asm]
                            row['frontend'] = 'clang (combined frontend/backend)'
                        else:
                            if route == 'c':
                                front = run([compiler, 'cc', *ktarget, '--emit-ir', src], optional=True)
                                stem.with_suffix('.frontend.keel').write_text(front['stdout'])
                                freeze(stem.with_suffix('.frontend.keel'))
                                row['frontend'] = 'passed' if front['returncode'] == 0 else 'failed'
                                if front['returncode'] != 0:
                                    failed = True; save(); continue
                            base = [compiler, 'compile' if route == 'ir' else 'cc', *ktarget, *opts]
                            objcmd = [*base, '--emit-object', src, '-o', obj]
                            asmcmd = [*base, src, '-o', asm]
                        built = run(objcmd, optional=True)
                        row['backend'] = ('passed' if built['returncode'] == 0 else
                                          'failed (combined Clang frontend/backend)' if variant == 'clang-O2' else
                                          'failed (C frontend preflight passed)' if route == 'c' else 'failed')
                        if built['returncode'] != 0:
                            failed = True; save(); continue
                        run(asmcmd)
                        row['code_size'] = {'text_bytes': len(text_bytes(obj)), 'object_bytes': obj.stat().st_size}
                        row['instructions'] = inspect_assembly(asm.read_text())
                        if a.objdump:
                            dis = run([a.objdump, '-d', obj])['stdout']
                            stem.with_suffix('.disasm').write_text(dis)
                        elif native and profile == 'apple-arm64':
                            stem.with_suffix('.disasm').write_text(run(['otool', '-tvV', obj])['stdout'])
                        elif native:
                            stem.with_suffix('.disasm').write_text(run(['objdump', '-d', obj])['stdout'])
                        row['disassembly'] = str(stem.with_suffix('.disasm')) if stem.with_suffix('.disasm').exists() else None
                        freeze(obj); freeze(asm)
                        if native:
                            run([clang, *ctarget, driver, obj, '-o', stem])
                            freeze(stem)
                            check = run([stem, 1, *(['unaligned'] if route == 'ir' else [])], data=data, optional=True)
                            row['correctness'] = 'passed' if check['returncode'] == 0 else 'failed'
                            failed |= check['returncode'] != 0
                        jobs.append((row, objcmd, stem, data))
                        save()
        # No measurements until every requested build and native check passes.
        if a.benchmark and not failed:
            rng = random.Random(a.seed)
            for round_index in range(a.rounds+1):
                verify()
                if a.baseline_keel: preflight()
                if a.baseline_keel:
                    groups = collections.defaultdict(list)
                    for job in jobs:
                        row = job[0]
                        groups[(row['profile'], row['case'], row['route'])].append(job)
                    keys = list(groups); rng.shuffle(keys)
                    ordered = []
                    for key in keys:
                        rng.shuffle(groups[key]); ordered.extend(groups[key])
                else:
                    rng.shuffle(jobs)
                    ordered = jobs
                for row, cmd, binary, data in ordered:
                    # Retain the primary object identity; timing writes a disposable sibling.
                    timing_cmd = [*cmd[:-1], str(binary)+'.timed.o']
                    sample = run(timing_cmd)
                    if round_index: row['compile_seconds'].append(sample['seconds'])
                if a.baseline_keel: preflight()
                native_jobs = [job for job in ordered if job[0]['native_allowed']]
                if not a.baseline_keel: rng.shuffle(native_jobs)
                for row, _, binary, data in native_jobs:
                    installed = a.output/'runtime'
                    shutil.copyfile(binary, installed); installed.chmod(0o755)
                    if sha(installed) != sha(binary): raise RuntimeError('runtime copy mismatch')
                    outer = time.perf_counter()
                    sample = json.loads(run([installed, a.repeats], data=data)['stdout'])
                    elapsed = time.perf_counter()-outer
                    if not math.isfinite(sample['seconds']) or not 0 < sample['seconds'] <= elapsed+0.005 or sample['calls'] != 64*a.repeats:
                        raise RuntimeError('invalid timer/work count')
                    if round_index: row['runtime_seconds'].append(sample['seconds'])
                if a.baseline_keel: preflight()
                verify()
                save()
        verify()
        report['complete'] = not failed
        report['conditions']['load_after'] = os.getloadavg()
    except Exception as exc:
        report['error'] = str(exc)
        failed = True
    finally:
        save()
    print(a.output/'report.json')
    return int(failed)


if __name__ == '__main__':
    raise SystemExit(main())
