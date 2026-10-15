"""Compare frozen Keel app/probe binaries with Clang O2 CPU/vectorizer ablations.

All Clang variants compile identical prepared .i inputs. Native flags therefore
change code generation/tuning, not source preprocessing. Both vectorizers are
disabled in the no-vector variants; explicit SIMD and libc SIMD can remain.
"""
import argparse
import collections
import concurrent.futures
import hashlib
import json
import math
import pathlib
import platform
import random
import re
import shutil
import statistics
import subprocess
import sys
import time

from measure_passes import summary
from profile_app_loops import APPS, invocation, text_bytes

ROOT = pathlib.Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / 'suite/scripts'))
from bench_vectorize import KERNELS, HARNESS


def run(command, **kwargs):
    result = subprocess.run(list(map(str, command)), capture_output=True, timeout=240, **kwargs)
    if result.returncode:
        raise RuntimeError(f'{command}: {result.returncode}\n{result.stderr.decode(errors="replace")}')
    return result


def sha(path):
    return hashlib.sha256(pathlib.Path(path).read_bytes()).hexdigest()


def compare(samples, numerator, denominator):
    return summary({'before': samples[denominator], 'after': samples[numerator]}, random.Random(42))


def measure(args, binaries, command, stdin=None, kernel=False):
    samples = {name: [] for name in binaries}
    names = list(binaries)
    expected = None if kernel else run(command('clang_o2'), input=stdin).stdout
    for iteration in range(args.rounds + 2):
        offset = iteration % len(names)
        order = names[offset:] + names[:offset]
        if (iteration // len(names)) % 2:
            order.reverse()
        for name in order:
            started = time.perf_counter_ns()
            result = run(command(name), input=stdin)
            elapsed = (time.perf_counter_ns() - started) / 1e6
            if kernel:
                # The self-checking C harness prints its measured duration.
                elapsed = float(result.stdout) * 1000
                assert math.isfinite(elapsed) and elapsed > 0
            else:
                assert result.stdout == expected, f'output mismatch: {name}'
            if iteration >= 2:
                samples[name].append(elapsed)
    comparisons = {name + '_over_clang_o2': compare(samples, name, 'clang_o2')
                   for name in samples if name != 'clang_o2'}
    for numerator, denominator in [('clang_o2', 'clang_o2_novec'),
                                   ('clang_native', 'clang_native_novec'),
                                   ('clang_native', 'clang_o2'),
                                   ('clang_native_novec', 'clang_o2_novec'),
                                   ('keel', 'clang_native'), ('keel_probe', 'clang_native')]:
        if numerator in samples and denominator in samples:
            comparisons[numerator + '_over_' + denominator] = compare(samples, numerator, denominator)
    return {'samples_ms': samples,
            'median_ms': {name: statistics.median(values) for name, values in samples.items()},
            'stdout_sha256': hashlib.sha256(expected).hexdigest() if expected is not None else None,
            'correctness': 'native harness checks every element and untouched prefix' if kernel else 'all stdout matches Clang O2',
            'measurement': 'inner kernel duration reported by native harness' if kernel else 'whole CLI elapsed milliseconds',
            'comparisons': comparisons}


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--compiler', type=pathlib.Path, required=True)
    p.add_argument('--profile', type=pathlib.Path, required=True)
    p.add_argument('--inline-probe', type=pathlib.Path, required=True)
    p.add_argument('--divide-probe', type=pathlib.Path, required=True)
    p.add_argument('--prepared', type=pathlib.Path, default=ROOT / 'target/suite')
    p.add_argument('--work-dir', type=pathlib.Path, required=True)
    p.add_argument('--rounds', type=int, default=12)
    p.add_argument('--jobs', type=int, default=4)
    p.add_argument('--cpu', type=int, default=0)
    p.add_argument('--build-only', action='store_true')
    p.add_argument('--reuse-builds', action='store_true')
    p.add_argument('--kernels-only', action='store_true')
    args = p.parse_args()
    if args.rounds < 6 or args.jobs < 1:
        p.error('use at least six rounds and one build worker')
    args.compiler = args.compiler.resolve(); args.prepared = args.prepared.resolve()
    work = args.work_dir.resolve(); work.mkdir(parents=True, exist_ok=True)
    output = work / 'comparison.json'
    native = '-mcpu=native' if platform.machine() in ['arm64', 'aarch64'] else '-march=native'
    common = ['-O2', '-ffp-contract=off']
    flags = {'clang_o2': common, 'clang_o2_novec': common + ['-fno-vectorize', '-fno-slp-vectorize'],
             'clang_native': common + [native],
             'clang_native_novec': common + [native, '-fno-vectorize', '-fno-slp-vectorize']}
    profile = json.loads(args.profile.read_text())
    assert sha(args.compiler) == profile['compiler_sha256']
    if args.reuse_builds:
        report = json.loads(output.read_text())
        for details in [*report['apps'].values(), *report['kernels'].values()]:
            for name, binary in details['binaries'].items():
                assert sha(binary) == details['binary_sha256'][name], binary
    else:
        report = {'host': platform.platform(), 'clang': run(['clang', '--version']).stdout.decode(),
                  'compiler_sha256': sha(args.compiler), 'script_sha256': sha(__file__),
                  'native_flag': native, 'flags': flags, 'drivers': {}, 'apps': {}, 'kernels': {},
                  'method': 'Identical prepared .i inputs; common -O2 -ffp-contract=off, no LTO/PGO/fast-math. Two warmup rounds then rotated/reversed configuration order. Same-round paired ratios and 95% paired bootstrap intervals (2000 samples), plus absolute medians and raw milliseconds. All builds/disassembly finish before timing. Linux timed commands pin CPU0; macOS uses normal scheduling. Probe binaries are diagnostic source/IR variants, not compiler optimizations. No-vector disables loop and SLP vectorizers, not explicit SIMD, backend copy lowering or SIMD in system libraries. Native flags apply after preprocessing: QuickJS libbf AVX2 source guards remain as in the common inputs.',
                  'timing_rounds': args.rounds, 'linux_pinned_cpu': args.cpu if platform.system() == 'Linux' else None}
        for name, options in flags.items():
            driver = run(['clang', *options, '-###', '-x', 'c', '-c', '/dev/null']).stderr.decode()
            (work / f'{name}-driver.txt').write_text(driver)
            cpu = re.search(r'"-target-cpu" "([^"\n]+)"', driver)
            report['drivers'][name] = {'cpu': cpu[1] if cpu else None,
                                      'loop_vectorizer': '"-vectorize-loops"' in driver,
                                      'slp_vectorizer': '"-vectorize-slp"' in driver,
                                      'features': re.findall(r'"-target-feature" "([^"\n]+)"', driver),
                                      'driver_path': str(work / f'{name}-driver.txt')}
        for app in APPS:
            directory = work / app; directory.mkdir(exist_ok=True)
            sources = sorted((args.prepared / app / 'pre').glob('*.i'))
            assert sources, app
            original = profile['apps'].get(app)
            if original:
                hashes = {u['source']: u['source_sha256'] for u in original['units']}
                assert all(sha(source) == hashes[str(source)] for source in sources)
                baseline = pathlib.Path(original['binaries']['normal'])
                assert sha(baseline) == original['binary_sha256']['normal']
            else:
                # LinuxHost did not need a Lua profile in the previous investigation.
                objects = []
                for source in sources:
                    asm, obj = directory/(source.stem+'.s'), directory/(source.stem+'.o')
                    run([args.compiler, 'cc', '-O', source, '-o', asm])
                    run(['clang', '-c', asm, '-o', obj]); objects.append(obj)
                baseline = directory/'keel'
                run(['clang', *objects, '-o', baseline, '-lm', '-lpthread'])
            binaries = {'keel': str(baseline)}
            probe_file = args.inline_probe if app == 'quickjs' else args.divide_probe if app == 'sqlite' else None
            if probe_file:
                probe = json.loads(probe_file.read_text())
                candidate = probe_file.resolve().parent/('qjs-expanded' if app == 'quickjs' else 'sqlite3-divide')
                assert sha(baseline) == probe['binary_sha256']['baseline']
                assert sha(candidate) == probe['binary_sha256']['candidate']
                binaries['keel_probe'] = str(candidate)
            evidence = {}
            for variant, options in flags.items():
                variant_dir = directory/variant; variant_dir.mkdir(exist_ok=True)
                def unit(source):
                    obj = variant_dir/(source.stem+'.o')
                    result = run(['clang', *options, '-Rpass=(loop-vectorize|slp-vectorizer)', '-c', source, '-o', obj])
                    remarks = variant_dir/(source.stem+'.remarks.txt'); remarks.write_bytes(result.stderr)
                    return obj, remarks
                with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as pool:
                    units = list(pool.map(unit, sources))
                binary = variant_dir/APPS[app][0]
                run(['clang', *[u[0] for u in units], '-o', binary, '-lm', '-lpthread'])
                binaries[variant] = str(binary)
                remarks = '\n'.join(p.read_text() for _, p in units)
                loops = [line for line in remarks.splitlines() if 'remark: vectorized loop' in line]
                slp = [line for line in remarks.splitlines() if 'remark:' in line and '[-Rpass=slp-vectorizer]' in line]
                evidence[variant] = {'loop_remark_count_nonunique': len(loops), 'slp_remark_count_nonunique': len(slp),
                                     'loop_widths': dict(collections.Counter(re.findall(r'vectorization width: (\d+)', remarks))),
                                     'remark_examples': (loops + slp)[:10],
                                     'remark_files': [{'path': str(path), 'sha256': sha(path)} for _, path in units]}
            for name, binary in binaries.items():
                command = ['xcrun', 'llvm-objdump', '-d', binary] if platform.system() == 'Darwin' else ['objdump', '-d', binary]
                disassembly = run(command).stdout.decode(errors='replace')
                evidence.setdefault(name, {}).update({'text_bytes': len(text_bytes(binary)),
                    'ymm_instruction_lines': sum(bool(re.search(r'\bymm\d+\b', line)) for line in disassembly.splitlines()),
                    'zmm_instruction_lines': sum(bool(re.search(r'\bzmm\d+\b', line)) for line in disassembly.splitlines())})
            report['apps'][app] = {'binaries': binaries, 'binary_sha256': {n: sha(b) for n,b in binaries.items()},
                                    'sources_sha256': {str(s): sha(s) for s in sources}, 'code_evidence': evidence}
            output.write_text(json.dumps(report, indent=2)+'\n')
            print(f'built {app}', flush=True)
        # A close arithmetic-map microbenchmark is reported separately from apps.
        for pattern in ['zero', 'range']:
            directory = work/('kernel-'+pattern); directory.mkdir(exist_ok=True)
            source, harness = directory/'kernel.c', directory/'harness.c'
            source.write_text('void map(float*d,const float*a,const float*b,unsigned long lo,unsigned long n){'+KERNELS[pattern]+'}\n')
            harness.write_text(HARNESS)
            binaries = {}; evidence = {}
            for name in ['keel', *flags]:
                obj = directory/(name+'.o'); binary = directory/name
                if name == 'keel':
                    run([args.compiler,'cc','-O','--emit-object',source,'-o',obj]); remarks = ''
                else:
                    result=run(['clang',*flags[name],'-Rpass=(loop-vectorize|slp-vectorizer)','-c',source,'-o',obj]);remarks=result.stderr.decode()
                run(['clang','-O2','-ffp-contract=off',harness,obj,'-o',binary])
                binaries[name]=str(binary);evidence[name]={'remarks':remarks,'text_bytes':len(text_bytes(obj))}
            report['kernels'][pattern]={'binaries':binaries,'binary_sha256':{n:sha(b) for n,b in binaries.items()},'elements':32768,'iterations':10000,'start_index':0 if pattern=='zero' else 7,'code_evidence':evidence}
        output.write_text(json.dumps(report,indent=2)+'\n')
    if args.build_only:
        return
    def pinned(command):
        return ['taskset','-c',str(args.cpu),*command] if platform.system()=='Linux' else command
    for app, details in ([] if args.kernels_only else report['apps'].items()):
        details['workloads']={}
        for bench in APPS[app][1]:
            fixture=ROOT/'suite'/app/('bench.sql' if app=='sqlite' else 'bench.lua' if app=='lua' else 'bench.js')
            _,stdin=invocation(app,details['binaries']['clang_o2'],fixture,bench)
            def command(name):return pinned(invocation(app,details['binaries'][name],fixture,bench)[0])
            details['workloads'][bench]=measure(args,details['binaries'],command,stdin)
            print(app,bench,{n:round(t,3) for n,t in details['workloads'][bench]['median_ms'].items()},flush=True)
        if app=='sqlite':
            fixture=(ROOT/'suite/sqlite/queries.sql').read_bytes()
            answers=[run([binary,':memory:'],input=fixture).stdout for binary in details['binaries'].values()]
            assert all(answer==answers[0] for answer in answers)
            details['sqlite_queries_match_all_variants']=True
        output.write_text(json.dumps(report,indent=2)+'\n')
    for pattern,details in report['kernels'].items():
        def command(name):return pinned([details['binaries'][name],str(details['elements']),str(details['iterations']),str(details['start_index'])])
        details['runtime']=measure(args,details['binaries'],command,kernel=True)
        print('kernel',pattern,details['runtime']['median_ms'],flush=True)
        output.write_text(json.dumps(report,indent=2)+'\n')
    report['kernel_measurement_script_sha256']=sha(__file__)
    report['kernel_timing_rounds']=args.rounds
    output.write_text(json.dumps(report,indent=2)+'\n')


if __name__=='__main__':
    main()
