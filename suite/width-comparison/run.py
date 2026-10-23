#!/usr/bin/env python3
"""Run ONLY under suite/fast_opt_locked.py; artifacts must be outside Git."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import random
import statistics
import subprocess
import sys
import time
from provenance import verify

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'suite/runtime-kernels'))
from oracle import inputs, MASK
import importlib.util
_spec = importlib.util.spec_from_file_location('runtime_kernel_runner', ROOT/'suite/runtime-kernels/run.py')
_runtime = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_runtime)
canonical_runtime = _runtime.canonical_runtime


def sha(p):
    return hashlib.sha256(Path(p).read_bytes()).hexdigest()


def paired_order(seed, rounds):
    rng = random.Random(seed)
    for r in range(rounds):
        arms = [128, 256]
        rng.shuffle(arms)
        yield r, arms


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--compiler', type=Path, required=True)
    ap.add_argument('--keel', type=Path, required=True)
    ap.add_argument('--output', type=Path, required=True)
    ap.add_argument('--cpu', type=int)
    ap.add_argument('--target', default='x86_64-unknown-linux-gnu')
    ap.add_argument('--rounds', type=int, default=9)
    ap.add_argument('--seed', type=int, default=20261023)
    ap.add_argument('--controls-only', action='store_true')
    a = ap.parse_args()
    if a.rounds < 7:
        ap.error('at least seven rounds required')
    if sys.platform == 'linux' and a.cpu is None and not a.controls_only:
        ap.error('Linux timing requires --cpu affinity')
    manifest_path = ROOT/'base-manifest.json'
    manifest = json.loads(manifest_path.read_text())
    base = verify(ROOT, manifest)
    out = a.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    if a.cpu is not None:
        os.sched_setaffinity(0, {a.cpu})
    report = dict(complete=False, base=base, base_manifest=manifest,
                  base_manifest_sha256=sha(manifest_path),
                  seed=a.seed, rounds=a.rounds, target=a.target,
                  hashes={str(p): sha(p) for p in [a.compiler, a.keel, Path(__file__),
                          ROOT/'suite/width-comparison/compiler.rs', ROOT/'suite/runtime-kernels/kernels.c',
                          ROOT/'suite/runtime-kernels/driver.c', ROOT/'suite/runtime-kernels/oracle.py',
                          ROOT/'suite/width-comparison/seeded.c', ROOT/'suite/width-comparison/guard.c']},
                  host=os.uname().nodename, affinity=sorted(os.sched_getaffinity(0)) if hasattr(os, 'sched_getaffinity') else None,
                  parity=[], samples=[], compile=[], preflight=[])
    def save():
        (out/'results.json').write_text(json.dumps(report, indent=2)+'\n')
    def run(cmd):
        cmd = list(map(str, cmd))
        start = time.perf_counter_ns()
        p = subprocess.run(cmd, capture_output=True, text=True)
        with (out/'commands.jsonl').open('a') as f:
            f.write(json.dumps(dict(argv=cmd, stdout=p.stdout, stderr=p.stderr,
                                   returncode=p.returncode, wall_ns=time.perf_counter_ns()-start))+'\n')
        if p.returncode:
            raise RuntimeError(f'{cmd}: {p.stderr}')
        return p.stdout
    def preflight():
        load = os.getloadavg()
        ticks = None
        if Path('/proc/stat').exists() and a.cpu is not None:
            def read():
                return list(map(int, next(x for x in Path('/proc/stat').read_text().splitlines()
                                         if x.startswith(f'cpu{a.cpu} ')).split()[1:]))
            before = read(); time.sleep(0.25); after = read()
            delta = [y-x for x, y in zip(before, after)]
            ticks = 1-(delta[3]+delta[4])/max(1, sum(delta[:8]))
        row = dict(unix=time.time(), load=load, cpu_busy=ticks)
        report['preflight'].append(row); save()
        if load[0] > 1 or (ticks is not None and ticks > .1):
            raise RuntimeError(f'contended host; timing deferred: {row}')
    try:
        report['tool_versions'] = {tool: run([tool, '--version']) for tool in ['clang']}
        for path in ['/proc/cpuinfo', f'/sys/devices/system/cpu/cpu{a.cpu}/cpufreq/scaling_governor',
                     f'/sys/devices/system/cpu/cpu{a.cpu}/cpufreq/scaling_driver',
                     f'/sys/devices/system/cpu/cpu{a.cpu}/topology/thread_siblings_list', '/proc/version']:
            if Path(path).exists(): report[path] = Path(path).read_text()
        report['processes_before'] = run(['ps', '-eo', 'pid,pcpu,comm,args'])
        source = out/'kernels.c'
        source.write_bytes((ROOT/'suite/runtime-kernels/kernels.c').read_bytes())
        profiles = ['arm'] if a.target.startswith('aarch64') else ['v1', 'v2', 'avx', 'v3']
        def compile_arm(profile, width, prefix):
            return json.loads(run([a.compiler, a.target, profile, width, source, prefix]))
        for profile in profiles:
            flags = [] if profile == 'arm' else ['--cpu-level', 'v3' if profile == 'avx' else profile]
            if profile == 'avx': flags += ['--target-features', '-avx2']
            cli = out/f'{profile}-cli'
            run([a.keel, 'cc', '-O', '--target', a.target, *flags, '--emit-object', source, '-o', str(cli)+'.o'])
            run([a.keel, 'cc', '-O', '--target', a.target, *flags, '--emit-ir', source, '-o', str(cli)+'.ir'])
            for width in [128, 256]:
                compile_arm(profile, width, out/f'{profile}-{width}')
            run([a.compiler, a.target, profile, 'pair', source, out/f'{profile}-pair'])
            for width in [128, 256]:
                for ext in ['o', 'ir']:
                    assert (out/f'{profile}-pair-{width}.{ext}').read_bytes() == (out/f'{profile}-{width}.{ext}').read_bytes(), 'same-process parity failed'
            assert Path(str(cli)+'.o').read_bytes() == (out/f'{profile}-128.o').read_bytes(), 'CLI object parity failed'
            assert Path(str(cli)+'.ir').read_bytes() == (out/f'{profile}-128.ir').read_bytes(), 'CLI IR parity failed'
            same = (out/f'{profile}-128.o').read_bytes() == (out/f'{profile}-256.o').read_bytes()
            assert same == (profile != 'v3'), f'unexpected activation: {profile}'
            report['parity'].append(dict(profile=profile, cli128=True, same_process=True, width_objects_equal=same))
            save()
        report['guards'] = []
        for profile in profiles:
            for width in [128, 256]:
                prefix = out/f'{profile}-{width}-seeded'
                run([a.compiler, a.target, profile, width, ROOT/'suite/width-comparison/seeded.c', prefix])
                binary = out/f'guard-{profile}-{width}'
                run(['clang', '-O2', '-fno-strict-aliasing', ROOT/'suite/width-comparison/guard.c',
                     out/f'{profile}-{width}.o', str(prefix)+'.o', '-o', binary])
                report['guards'].append(dict(profile=profile, width=width, **json.loads(run([binary]))))
                save()
        if a.controls_only:
            report['complete'] = True; save(); return
        assert a.target == 'x86_64-unknown-linux-gnu' and a.cpu is not None
        run(['clang', '-O2', '-c', ROOT/'suite/runtime-kernels/driver.c', '-o', out/'driver.o'])
        for profile in profiles:
            for width in [128, 256]:
                obj = out/f'{profile}-{width}.o'
                report['hashes'][str(obj)] = sha(obj)
                (out/f'{profile}-{width}.dis').write_text(run(['objdump', '-d', '-Mintel', obj]))
        binaries = {}
        for width in [128, 256]:
            binaries[width] = out/f'run-{width}'
            run(['clang', out/'driver.o', out/f'v3-{width}.o', '-o', binaries[width]])
            report['hashes'][str(binaries[width])] = sha(binaries[width])
        lengths = list(range(82))+[257,4096,65536,1048576]
        expected = {n: sum(inputs(n, a.seed)[0]) & MASK for n in lengths}
        for width in [128,256]:
            for n in lengths:
                run([canonical_runtime(binaries[width], out), 'check', 'widen32', n, 1, a.seed, expected[n]])
        report['correctness'] = dict(lengths=lengths, arms=[128,256]); save()
        # Compile times are separate, same-process service time; fresh-process wall is in commands.
        for r, arms in paired_order(a.seed, a.rounds+1):
            preflight()
            for width in arms:
                result = compile_arm('v3', width, out/f'compile-{width}')
                if r: report['compile'].append(dict(round=r-1, width=width, **result))
            preflight()
        for r, arms in paired_order(a.seed, a.rounds+1):
            preflight()
            for n in [8,17,257,4096,65536,1048576]:
                for width in arms:
                    result = json.loads(run([canonical_runtime(binaries[width], out), 'time', 'widen32', n,
                                             min(1000000,max(256, 67108864//n)), a.seed, expected[n]]))
                    if r: report['samples'].append(dict(round=r-1, width=width, **result))
            preflight()
            save()
        report['summary'] = {}
        for n in [8,17,257,4096,65536,1048576]:
            med = {w: statistics.median(x['elapsed_ns']/x['reps'] for x in report['samples']
                                       if x['width']==w and x['length']==n) for w in [128,256]}
            ratios = []
            for r in range(a.rounds):
                rows = {x['width']: x['elapsed_ns']/x['reps'] for x in report['samples'] if x['round']==r and x['length']==n}
                ratios.append(rows[128]/rows[256])
            report['summary'][n] = dict(median_ns=med, paired_ratios=ratios)
        report['complete'] = True; save()
    finally:
        report['processes_after'] = run(['ps', '-eo', 'pid,pcpu,comm,args'])
        save()


if __name__ == '__main__':
    main()
