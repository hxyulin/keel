#!/usr/bin/env python3
"""Compare native command-line assemblers and linkers on macOS or Linux."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import random
import shutil
import statistics
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'suite/asm'))
from audit_clang_assembler import macho


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def run(argv, *, quiet=False):
    p = subprocess.run(list(map(str, argv)), capture_output=True, timeout=60)
    if p.returncode:
        raise RuntimeError(f'{argv}: {p.returncode}\n{p.stderr.decode(errors="replace")}')
    require(not p.stderr, f'{argv}: unexpected stderr: {p.stderr.decode(errors="replace")}')
    require(not quiet or not p.stdout, f'{argv}: unexpected stdout: {p.stdout.decode(errors="replace")}')
    return p.stdout.decode()


def linux_link(a, out, keel):
    """Link the same Clang-built objects and glibc startup files with each tool."""
    clang = Path(shutil.which('clang')).resolve()
    tools = {'keel': keel}
    for name in ('ld.bfd', 'ld.lld', 'mold'):
        if path := shutil.which(name):
            tools[name] = Path(path).absolute()
    require(len(tools) >= 2, 'install an ELF comparison linker')
    triple = 'x86_64-unknown-linux-gnu'
    interpreter = '/lib64/ld-linux-x86-64.so.2'
    require(Path(interpreter).is_file(), 'the glibc ELF interpreter is required')
    report = {
        'schema': 1,
        'source_commit': run(['git', 'rev-parse', 'HEAD']).strip(),
        'platform': platform.platform(),
        'cpu': next(line.split(':', 1)[1].strip() for line in Path('/proc/cpuinfo').read_text().splitlines()
                    if line.startswith('model name')),
        'affinity': sorted(os.sched_getaffinity(0)),
        'load_before': os.getloadavg(),
        'clang_version': run([clang, '--version']),
        'harness_sha256': sha(__file__),
        'warmups': 2,
        'rounds': 11,
        'seed': 20261024,
    }
    sources = {
        'kernels': ROOT / 'crates/keel/tests/fixtures/runtime-kernels.c',
        'driver': ROOT / 'suite/linker/fixtures/driver.c',
    }
    application_objects = []
    build_commands = []
    for name, source in sources.items():
        obj = out / f'{name}.o'
        command = [str(clang), '-target', triple, '-O0', '-fPIC', '-c', str(source), '-o', str(obj)]
        run(command, quiet=True)
        build_commands.append(command)
        application_objects.append(obj)

    def glibc_file(name):
        path = Path(run([clang, f'-print-file-name={name}']).strip())
        require(path.is_absolute() and path.is_file(), f'Clang did not locate {name}')
        return path.resolve()

    # Use the actual DSO rather than the distribution's libc.so linker script.
    inputs = [glibc_file('crt1.o'), glibc_file('crti.o'), *application_objects,
              glibc_file('libc.so.6'), glibc_file('crtn.o')]
    commands = {name: [str(tool), *(['link', '--target', triple] if name == 'keel' else []),
                       '--dynamic-linker', interpreter, *map(str, inputs), '-o', str(out / 'program')]
                for name, tool in tools.items()}
    report['build_commands'] = build_commands
    report['sources'] = {str(p.relative_to(ROOT)): sha(p) for p in sources.values()}
    report['tools'] = {'keel': {'sha256': sha(keel), 'version': report['source_commit']}}
    report['tools'].update({name: {'sha256': sha(tool), 'version': run([tool, '--version']).strip()}
                            for name, tool in tools.items() if name != 'keel'})
    frozen_inputs = {p: sha(p) for p in inputs}
    report['objects'] = [{'name': p.name, 'sha256': digest, 'bytes': p.stat().st_size}
                         for p, digest in frozen_inputs.items()]

    sys.path.insert(0, str(ROOT / 'suite/linker/fixtures'))
    import oracle

    seed = report['seed']
    lengths = (0, 1, 33, 257)
    answers = [(n, oracle.reference(*oracle.inputs(n, seed))) for n in lengths]
    verified = {}
    for name, command in commands.items():
        exe = out / f'link-{name}'
        run([*command[:-1], exe], quiet=True)
        for n, cases in answers:
            for case, expected in cases.items():
                result = json.loads(run([exe, 'check', case, n, 1, seed, expected]))
                require(result['result'] == expected, f'{name}: {case} length {n}: oracle mismatch')
        verified[name] = {'bytes': exe.stat().st_size, 'sha256': sha(exe)}
    report['correctness'] = {'passed': True, 'checks_per_executable': sum(len(c) for _, c in answers),
                             'lengths': lengths, 'verified_output': verified}

    orders = []
    samples = {name: [] for name in commands}
    rng = random.Random(seed)
    if not a.correctness_only:
        for round_no in range(report['warmups'] + report['rounds']):
            order = list(commands)
            rng.shuffle(order)
            orders.append(order)
            for name in order:
                start = time.perf_counter_ns()
                run(commands[name], quiet=True)
                elapsed = time.perf_counter_ns() - start
                if round_no >= report['warmups']:
                    samples[name].append(elapsed / 1e6)
    report['link'] = {'commands': commands, 'orders_including_warmups': orders, 'samples_ms': samples,
                      'median_ms': {name: statistics.median(values) for name, values in samples.items() if values}}
    report['load_after'] = os.getloadavg()
    for path, digest in frozen_inputs.items():
        require(sha(path) == digest, f'input changed during timing: {path}')
    for name, path in tools.items():
        require(sha(path) == report['tools'][name]['sha256'], f'linker changed during timing: {name}')
    (out / 'raw.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report['correctness'] if a.correctness_only else report['link']['median_ms'], indent=2))


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--keel', type=Path, required=True)
    ap.add_argument('--out', type=Path, required=True)
    ap.add_argument('--correctness-only', action='store_true', help='run all gates without collecting timings')
    a = ap.parse_args()
    out = a.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    keel = a.keel.resolve()
    if (platform.system(), platform.machine()) == ('Linux', 'x86_64'):
        linux_link(a, out, keel)
        return
    clang = Path(shutil.which('clang')).resolve()
    selected_clang = Path(run(['xcrun', '--find', 'clang']).strip()).resolve()
    ld = Path(run(['xcrun', '--find', 'ld']).strip()).resolve()
    sdk = Path(run(['xcrun', '--sdk', 'macosx', '--show-sdk-path']).strip())
    triple = 'arm64-apple-macosx11.0.0'
    report = {'schema': 1, 'date': time.strftime('%Y-%m-%d'), 'source_commit': run(['git', 'rev-parse', 'HEAD']).strip(),
              'host': platform.node(), 'platform': platform.platform(),
              'cpu': run(['sysctl', '-n', 'machdep.cpu.brand_string']).strip(),
              'clang_version': run([clang, '--version']), 'apple_ld_version': subprocess.run([str(ld), '-v'], capture_output=True, text=True, check=True).stderr,
              'sdk': str(sdk), 'sdk_version': run(['xcrun', '--sdk', 'macosx', '--show-sdk-version']).strip(),
              'tools': {n: {'path': str(p), 'sha256': sha(p)} for n, p in [('keel', keel), ('clang', clang), ('ld', ld), ('selected_clang', selected_clang)]},
              'correctness_only': a.correctness_only,
              'dependencies': {str(p): sha(ROOT / p) for p in ('suite/asm/audit_clang_assembler.py', 'suite/linker/fixtures/oracle.py', 'suite/common/fast_opt_locked.py')},
              'harness_sha256': sha(__file__), 'warmups': 2, 'rounds': 7, 'seed': 20261024,
              'load_before': os.getloadavg(), 'preparation_commands': [], 'sources': [],
              'assembly': {}, 'link': {}, 'correctness': {}}
    def prepare(cmd, *, quiet=True):
        report['preparation_commands'].append(list(map(str, cmd)))
        return run(cmd, quiet=quiet)
    # Clang generation is preparation only; every assembler consumes identical text.
    for name in ('kernels', 'driver'):
        src = (ROOT / 'crates/keel/tests/fixtures/runtime-kernels.c' if name == 'kernels' else ROOT / 'suite/linker/fixtures/driver.c')
        asm = out / (name + '.s')
        prepare([clang, '-S', '-O0', '-target', triple, '-mcpu=apple-m1', '-isysroot', sdk, src, '-o', asm])
        report['sources'].append({'path': str(src.relative_to(ROOT)), 'sha256': sha(src),
                                  'assembly_path': str(asm), 'assembly_sha256': sha(asm),
                                  'assembly_bytes': asm.stat().st_size})
    asm_commands = {}
    for mode in ('keel', 'clang-cc1as', 'clang-driver'):
        commands = []
        for name in ('kernels', 'driver'):
            src, obj = out / (name + '.s'), out / (name + '-' + mode + '.o')
            if mode == 'keel':
                cmd = [keel, 'asm', '--target', 'aarch64-apple-darwin', '--cpu', 'baseline', src, '-o', obj]
            elif mode == 'clang-cc1as':
                cmd = [clang, '-cc1as', '-triple', triple, '-target-cpu', 'apple-m1', '-filetype', 'obj', src, '-o', obj]
            else:
                cmd = [clang, '-target', triple, '-mcpu=apple-m1', '-x', 'assembler', '-c', src, '-o', obj]
            commands.append(list(map(str, cmd)))
            prepare(cmd)
        asm_commands[mode] = commands
    # Compare exact instruction bytes and normalized relocation targets before execution.
    checks = []
    for name in ('kernels', 'driver'):
        reference = macho(out / (name + '-clang-cc1as.o'))[0]
        for mode in asm_commands:
            actual = macho(out / (name + '-' + mode + '.o'))[0]
            reftext = next(s for s in reference if s['name'] == '__TEXT,__text')
            text = next(s for s in actual if s['name'] == '__TEXT,__text')
            require(text['data'] == reftext['data'], f'{name}: {mode}: instruction bytes differ')
            require(text['relocations'] == reftext['relocations'], f'{name}: {mode}: text relocations differ')
            checks.append({'input': name, 'mode': mode, 'text_bytes': len(text['data']),
                           'text_sha256': hashlib.sha256(text['data']).hexdigest(),
                           'normalized_text_relocations_equal': True})
    report['correctness']['instructions'] = checks
    objects = [out / (n + '-clang-cc1as.o') for n in ('kernels', 'driver')]
    exe = out / 'program'
    link_commands = {
        'keel': [keel, 'link', '--sysroot', sdk, '-mmacosx-version-min=11.0', *objects, '-o', exe],
        'keel-dead-strip': [keel, 'link', '--dead-strip', '--sysroot', sdk, '-mmacosx-version-min=11.0', *objects, '-o', exe],
        'apple-ld': [ld, '-arch', 'arm64', '-platform_version', 'macos', '11.0', report['sdk_version'], '-syslibroot', sdk, '-e', '_main', *objects, '-lSystem', '-o', exe],
        'apple-ld-dead-strip': [ld, '-arch', 'arm64', '-platform_version', 'macos', '11.0', report['sdk_version'], '-syslibroot', sdk, '-e', '_main', '-dead_strip', *objects, '-lSystem', '-o', exe],
        'clang-driver': [clang, '-target', triple, '-isysroot', sdk, *objects, '-o', exe],
    }
    link_commands = {k: list(map(str, v)) for k, v in link_commands.items()}
    spec = importlib.util.spec_from_file_location('oracle', ROOT / 'suite/linker/fixtures/oracle.py')
    oracle = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(oracle)
    answers = [(n, oracle.reference(*oracle.inputs(n, 20261024))) for n in (0, 1, 33, 257)]
    def validate(executable):
        prepare(['codesign', '--verify', '--strict', executable])
        for n, values in answers:
            for case, expected in values.items():
                result = json.loads(prepare([executable, 'check', case, n, 1, 20261024, expected], quiet=False))
                require(result['result'] == expected, f'{executable}: {case}: oracle mismatch')
    # All assemblers are executed through the same Apple driver; linkers use identical objects.
    for mode in asm_commands:
        path = out / ('assembly-' + mode)
        prepare([clang, '-target', triple, '-isysroot', sdk,
                 *[out / (n + '-' + mode + '.o') for n in ('kernels', 'driver')], '-o', path])
        validate(path)
    link_outputs = {}
    for mode, cmd in link_commands.items():
        checked_exe = out / ('link-' + mode)
        prepare([*cmd[:-1], checked_exe])
        validate(checked_exe)
        link_outputs[mode] = {'path': str(checked_exe), 'bytes': checked_exe.stat().st_size, 'sha256': sha(checked_exe)}
    report['correctness']['oracle'] = {'cases': list(oracle.CASES), 'lengths': [n for n, _ in answers],
        'seed': 20261024, 'checks_per_executable': 48, 'assembly_executables': 3, 'link_executables': len(link_commands),
        'all_passed': True, 'strict_signatures_passed': True, 'oracle_sha256': sha(ROOT / 'suite/linker/fixtures/oracle.py')}
    rng = random.Random(20261024)
    frozen_inputs = {p: sha(p) for p in [*objects, *[out / (n + '.s') for n in ('kernels', 'driver')]]}
    stages = [('assembly', asm_commands), ('link', {k: [v] for k, v in link_commands.items()})]
    for stage, commands in ([] if a.correctness_only else stages):
        samples = {k: [] for k in commands}
        orders = []
        for i in range(9):
            order = list(commands)
            rng.shuffle(order)
            orders.append(order)
            for mode in order:
                start = time.perf_counter_ns()
                for cmd in commands[mode]:
                    run(cmd, quiet=True)
                elapsed = (time.perf_counter_ns() - start) / 1e6
                if i >= 2:
                    samples[mode].append(elapsed)
        report[stage] = {'commands': commands, 'orders_including_warmups': orders,
                         'samples_ms': samples, 'median_ms': {k: statistics.median(v) for k, v in samples.items()}}
    report['link']['verified_output'] = link_outputs
    report['objects'] = [{'path': str(p), 'sha256': sha(p), 'bytes': p.stat().st_size} for p in objects]
    report['load_after'] = os.getloadavg()
    for identity in report['tools'].values():
        require(sha(identity['path']) == identity['sha256'], f'tool changed during campaign: {identity["path"]}')
    for path, digest in frozen_inputs.items():
        require(sha(path) == digest, f'input changed during campaign: {path}')
    (out / 'raw.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({'correctness': report['correctness']['oracle']} if a.correctness_only else
                     {s: report[s]['median_ms'] for s in ('assembly', 'link')}, indent=2))


if __name__ == '__main__':
    main()
