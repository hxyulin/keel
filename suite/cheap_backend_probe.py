#!/usr/bin/env python3
"""Small Apple ARM backend probe; prebuilt binaries, fresh output, external lease.

Complements crates/keel-cranelift/tools/compare.py with the actual Fast IR tier
and an LLVM-compatible scalar fixture. LLVM CLI times include process startup
and LLVM text parsing, so are deliberately separate from in-process timings.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import random
import statistics
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]
p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--keel', type=Path, required=True)
p.add_argument('--bench', type=Path, required=True)
p.add_argument('--bridge', type=Path, required=True)
p.add_argument('--exporter', type=Path, required=True)
p.add_argument('--comparison', type=Path, required=True)
p.add_argument('--out', type=Path, required=True)
p.add_argument('--reps', type=int, default=9)
p.add_argument('--rounds', type=int, default=3)
a = p.parse_args()
if a.reps < 1 or a.rounds < 1:
    p.error('positive sample counts required')
out = a.out.resolve()
out.mkdir(parents=True, exist_ok=False)
commands = []
result = dict(commands=commands, head=subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
              binaries={str(x): hashlib.sha256(x.read_bytes()).hexdigest()
                        for x in [a.keel, a.bench, a.bridge, a.exporter]},
              reps=a.reps, rounds=a.rounds)


def save():
    (out / 'results.json').write_text(json.dumps(result, indent=2) + '\n')


def run(*argv):
    argv = list(map(str, argv))
    start = time.perf_counter_ns()
    c = subprocess.run(argv, capture_output=True)
    elapsed = time.perf_counter_ns() - start
    commands.append(dict(argv=argv, rc=c.returncode, ns=elapsed,
                         stdout=c.stdout.decode(errors='replace'), stderr=c.stderr.decode(errors='replace')))
    if c.returncode:
        save()
        raise RuntimeError(commands[-1])
    return c.stdout.decode(), elapsed


rng = random.Random(20261020)
fixture = ROOT / 'crates/keel-llvm/tests/fixtures'
native = ROOT / 'crates/keel-cranelift'
base = a.comparison.resolve()
# Generate Fast IR once. No pass time is included in the backend-only timing.
fast = out / 'kernels.fast.keel'
run(a.keel, 'cc', '--opt-tier', 'fast', '--emit-ir', native / 'kernels/kernels.c', '-o', fast)
records, _ = run(a.bench, 'objects', '--out-dir', out, '--label', 'kernels.fast', fast)
oracle = (base / 'correctness/kernels.clang-O0.out').read_text()
fast_exes = {}
for line in records.splitlines():
    r = json.loads(line)
    exe = out / ('fast-' + r['config'])
    run('clang', base / 'bin/driver.o', r['path'], '-o', exe)
    text, _ = run(exe, 'check')
    assert text == oracle, r['config']
    fast_exes[r['config']] = exe
run(a.bench, 'time', '--warmup', 2, '--reps', a.reps, '--out', out / 'fast-compile.json',
    '--emit-dir', out, 'kernels.fast=' + str(fast))
manifest = json.loads((base / 'manifest.json').read_text())
runtime = []
for rnd in range(a.rounds):
    for kernel, reps in manifest['protocol']['kernel_reps'].items():
        # Repeat the Full, CLIF-speed and Clang controls alongside Fast.
        arms = {'fast-native': fast_exes['keel-full'],
                'fast-clif-speed': fast_exes['clif-speed'],
                'full-native': base / 'bin/O-keel-full',
                'full-clif-speed': base / 'bin/O-clif-speed',
                'clang-O2': base / 'bin/clang-O2'}
        order = list(arms)
        rng.shuffle(order)
        checks = []
        for arm in order:
            text, _ = run(arms[arm], 'time', kernel, reps)
            fields = text.split()
            checks.append(fields[2])
            runtime.append(dict(round=rnd, kernel=kernel, arm=arm, reps=reps, ns=int(fields[1])))
        assert len(set(checks)) == 1, (kernel, checks)
result['runtime'] = runtime
medians = {(k, arm): statistics.median(r['ns'] / r['reps'] for r in runtime
                                     if r['kernel'] == k and r['arm'] == arm)
           for k in manifest['protocol']['kernel_reps'] for arm in arms}
result['runtime_vs_clang'] = {arm: math.exp(statistics.mean(math.log(medians[k, arm] / medians[k, 'clang-O2'])
                                      for k in manifest['protocol']['kernel_reps'])) for arm in arms}
# Use all four functions from the checked-in scalar LLVM fixture unchanged.
hot_ir = out / 'hot.raw.keel'
run(a.keel, 'cc', '--emit-ir', fixture / 'hot.c', '-o', hot_ir)
ll = out / 'hot.ll'
run(a.exporter, '--target', 'arm64-apple-macosx11.0.0', hot_ir, '-o', ll)
compile_arms = {
    'keel-none': [a.keel, 'compile', '--emit-object', hot_ir],
    'keel-fast': [a.keel, 'compile', '--opt-tier', 'fast', '--isel', 'all', '--emit-object', hot_ir],
    'keel-full': [a.keel, 'compile', '-O', '--emit-object', hot_ir],
    'clif-none': [a.bridge, '--target', 'arm64-apple-macosx11.0.0', '--opt', 'none', hot_ir],
    'clif-speed': [a.bridge, '--target', 'arm64-apple-macosx11.0.0', '--opt', 'speed', hot_ir],
    'llvm-O0': ['clang', '-Wno-override-module', '-O0', '-c', ll],
    'llvm-O2': ['clang', '-Wno-override-module', '-O2', '-c', ll],
    'clang-O2-source': ['clang', '-O2', '-ffp-contract=off', '-c', fixture / 'hot.c'],
}
exes = {}
outputs = {}
for arm, cmd in compile_arms.items():
    obj = out / (arm + '.o')
    run(*cmd, '-o', obj)
    exe = out / (arm + '.check')
    run('clang', '-O2', fixture / 'hot_driver.c', obj, '-o', exe)
    outputs[arm], _ = run(exe)
    (out / (arm + '.stdout')).write_text(outputs[arm])
assert len(set(outputs.values())) == 1, 'scalar differential check failed'
result['scalar_correctness'] = dict(builds=len(outputs), lines=next(iter(outputs.values())).count('\n'), passed=True)
# A single externally called loop: no LTO, identical driver, startup excluded.
driver = out / 'timer.c'
driver.write_text('''#include <stdint.h>
#include <stdio.h>
#include <time.h>
extern unsigned long hot(unsigned long*,unsigned long,unsigned long);
int main(void) {
 unsigned long a[4096],sum=0; for(unsigned long i=0;i<4096;i++)a[i]=i*2357ul;
 struct timespec t,u; clock_gettime(CLOCK_UPTIME_RAW,&t);
 for(unsigned long i=0;i<40000;i++)sum+=hot(a,4096,i);
 clock_gettime(CLOCK_UPTIME_RAW,&u);
 printf("%llu %lu\\n",(unsigned long long)((u.tv_sec-t.tv_sec)*1000000000ll+u.tv_nsec-t.tv_nsec),sum);
}''')
for arm in compile_arms:
    exes[arm] = out / (arm + '.time')
    run('clang', '-O2', driver, out / (arm + '.o'), '-o', exes[arm])
times = {arm: [] for arm in compile_arms}
for rnd in range(a.reps + 1):
    order = list(compile_arms)
    rng.shuffle(order)
    for arm in order:
        _, ns = run(*compile_arms[arm], '-o', out / (arm + '.timed.o'))
        if rnd:
            times[arm].append(ns)
result['scalar_cli_ns'] = times
hot_runtime = {arm: [] for arm in compile_arms}
for rnd in range(a.rounds + 1):
    order = list(exes)
    rng.shuffle(order)
    checks = []
    for arm in order:
        text, _ = run(exes[arm])
        ns, check = text.split()
        checks.append(check)
        if rnd:
            hot_runtime[arm].append(int(ns))
    assert len(set(checks)) == 1, checks
result['scalar_runtime_ns'] = hot_runtime
result['completed'] = True
save()
print(json.dumps({k: result[k] for k in ['runtime_vs_clang', 'scalar_correctness']}, indent=2))
print('scalar median CLI ms', {k: round(statistics.median(v)/1e6, 3) for k,v in times.items()})
print('scalar median runtime ms', {k: round(statistics.median(v)/1e6, 3) for k,v in hot_runtime.items()})
