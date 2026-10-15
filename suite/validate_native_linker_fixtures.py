#!/usr/bin/env python3
"""Small native/Apple comparison before running the application suites."""
import argparse
import json
from pathlib import Path
import subprocess


def run(*argv):
    p = subprocess.run([str(a) for a in argv], capture_output=True)
    if p.returncode:
        raise RuntimeError(f'{argv}: exit {p.returncode}\n{p.stdout.decode(errors="replace")}\n{p.stderr.decode(errors="replace")}')
    return p.stdout


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--keel', type=Path, required=True)
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--profile-gate', type=Path, help='defer work unless this coordination file exists')
    a = p.parse_args()
    if a.profile_gate and not a.profile_gate.exists():
        p.error(f'profiling gate is closed: {a.profile_gate}')
    keel, out = a.keel.resolve(), a.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    fixtures = Path(__file__).resolve().parent / 'native-linker'
    for name in ['pointers', 'other', 'backtrace', 'aslr']:
        run(keel, 'cc', '--emit-object', fixtures / (name + '.c'), '-o', out / (name + '.o'))
    run('ar', 'rcs', out / 'libother.a', out / 'other.o')
    run(keel, 'compile', '--emit-object', fixtures / 'exceptions.keel', '-o', out / 'exceptions.o')
    run('clang++', '-O2', '-c', fixtures / 'exceptions.cpp', '-o', out / 'exception-driver.o')
    run(keel, 'asm', fixtures / 'addends.s', '-o', out / 'addends.o')
    run(keel, 'asm', fixtures / 'compact.s', '-o', out / 'compact.o')
    run('clang++', '-O2', '-c', fixtures / 'compact.cpp', '-o', out / 'compact-driver.o')
    results = {}
    for name, objects, args, libs in [
        ('pointers', ['pointers.o', 'other.o'], ['one', 'two'], []),
        ('archive', ['pointers.o', 'libother.a'], ['one', 'two'], []),
        ('addends', ['addends.o'], [], []),
        ('backtrace', ['backtrace.o'], [], []),
        ('exceptions', ['exceptions.o', 'exception-driver.o'], [], ['-lc++']),
        ('compact', ['compact.o', 'compact-driver.o'], [], ['-lc++']),
    ]:
        objects = [out / obj for obj in objects]
        native, apple = out / name, out / (name + '-apple')
        run(keel, 'link', *objects, *libs, '-o', native)
        run('clang', *objects, *libs, '-o', apple)
        expected = run(apple, *args)
        for _ in range(5):
            assert run(native, *args) == expected, name
        for exe in [native, apple]:
            run('codesign', '--verify', '--strict', exe)
            for tool, flags in [('otool', ['-l']), ('nm', ['-nm']),
                                ('dyld_info', ['-imports', '-exports', '-fixups']),
                                ('dwarfdump', ['--eh-frame'])]:
                (out / (exe.name + '.' + tool + '.txt')).write_bytes(run(tool, *flags, exe))
        results[name] = {'native_launches': 5, 'apple_output_matches': True}
        print(f'{name}: native and Apple outputs agree; five native launches pass')

    run(keel, 'link', out / 'aslr.o', '-o', out / 'aslr')
    addresses = {run(out / 'aslr').strip().decode() for _ in range(5)}
    assert len(addresses) > 1, 'PIE did not slide across launches'
    results['aslr'] = {'launches': 5, 'distinct_addresses': len(addresses)}
    (out / 'results.json').write_text(json.dumps(results, indent=2) + '\n')
    print(f'ASLR: {len(addresses)} distinct main addresses across five launches')

if __name__ == '__main__':
    main()
