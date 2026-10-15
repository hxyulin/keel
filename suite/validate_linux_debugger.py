#!/usr/bin/env python3
"""Check native Linux/GDB variables, scopes, TLS and state mutation for both allocators.

Requires an x86_64 Linux keel binary, Clang, GDB and llvm-dwarfdump.
The Clang reference and Fast/Full builds, with and without IR optimization,
run the same session. The fixture finishes with status 183 after mutations.
"""
import argparse
import json
from pathlib import Path
import subprocess


def run(command, **kwargs):
    return subprocess.run(list(map(str, command)), check=True, capture_output=True,
                          text=True, timeout=90, **kwargs)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--compiler', type=Path, required=True)
    parser.add_argument('--work-dir', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    args.work_dir.mkdir(parents=True, exist_ok=True)
    source = Path(__file__).resolve().parent.parent / 'crates/keel-c/tests/debug/vars.c'
    preprocessed = args.work_dir / 'vars.i'
    run(['clang', '-E', '-std=c11', source, '-o', preprocessed])
    commands = args.work_dir / 'session.gdb'
    commands.write_text(r'''set pagination off
set confirm off
set disable-randomization off
break stop
run
up
printf "MAIN_P=%d,%d\n", p.x, p.y
printf "MAIN_NODE=%d FLAGS=%u,%u,%d UNION=%.2f\n", a.next->value, fl.a, fl.b, fl.c, w.d
printf "MAIN_EXPR=%d TLS=%d ENUM=%d LEN=%lu\n", arr[1] + *ptr, per_thread, c, len
set var d = 10.5
continue
up
printf "LOOP_I=%d SQ=%d TOTAL=%d\n", i, sq, total
set var total = 100
continue
up
printf "INNER_TOTAL=%d\n", total
finish
continue
printf "EXIT=%d\n", $_exitcode
''')
    expected = ['MAIN_P=3,4', 'MAIN_NODE=2 FLAGS=5,17,-3 UNION=0.25',
                'MAIN_EXPR=5 TLS=8 ENUM=5 LEN=16', 'LOOP_I=2 SQ=4 TOTAL=3',
                'INNER_TOTAL=99', 'EXIT=183']
    report = {'source': str(source), 'expected_markers': expected, 'builds': {}}
    for name, allocation, optimized in [('clang', None, False), ('full', 'full', False),
                                       ('full-opt', 'full', True), ('fast', 'fast', False),
                                       ('fast-opt', 'fast', True)]:
        executable = args.work_dir / name
        if allocation is None:
            run(['clang', '-O0', '-g', '-std=c11', source, '-o', executable])
        else:
            assembly = args.work_dir / (name + '.s')
            command = [args.compiler.resolve(), 'cc', preprocessed.resolve(), '-g', '--regalloc', allocation, '-o', assembly.resolve()]
            if optimized:
                command.append('-O')
            run(command)
            run(['clang', '-g', assembly, '-o', executable])
        dwarf = run(['llvm-dwarfdump', '--verify', executable])
        session = run(['gdb', '--batch', '--quiet', '-x', commands, executable])
        (args.work_dir / (name + '.gdb.log')).write_text(session.stdout + session.stderr)
        for marker in expected:
            if marker not in session.stdout:
                raise RuntimeError(f'{name}: missing {marker!r}\n{session.stdout}\n{session.stderr}')
        report['builds'][name] = {'debugger_passed': True, 'dwarf_verified': True,
                                  'gdb': session.stdout + session.stderr, 'dwarf': dwarf.stdout + dwarf.stderr}
        print(name, 'GDB and DWARF passed', flush=True)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + '\n')


if __name__ == '__main__':
    main()
