#!/usr/bin/env python3
"""Native-vs-Apple link validation, gated to avoid the parent's timing session.

Use --cached-root <repo>/target/suite to reuse prior
Keel assembly, or --compile to compile cached .i inputs with this Keel first.
All new artifacts go under --out; the cached build tree is read-only.
"""
import argparse
import json
import os
import shutil
from pathlib import Path
import subprocess


def run(argv, **kw):
    return subprocess.run([str(x) for x in argv], check=True, **kw)

def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--keel', type=Path, required=True)
    p.add_argument('--cached-root', type=Path, required=True)
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--compile', action='store_true')
    p.add_argument('--plain', action='store_true', help='use unoptimized Keel output')
    p.add_argument('--profile-gate', type=Path, help='defer work unless this coordination file exists')
    a = p.parse_args()
    if a.profile_gate and not a.profile_gate.exists():
        p.error(f'profiling gate is closed: {a.profile_gate}')
    a.keel = a.keel.resolve()
    a.out = a.out.resolve()
    a.out.mkdir(parents=True, exist_ok=True)
    report = {}
    for suite, binary, source in [
        ('lua', 'lua', 'lua-5.4.7-tests'),
        ('quickjs', 'qjs', 'quickjs-2024-01-13'),
        ('sqlite', 'sqlite3', 'sqlite-amalgamation-3460100'),
    ]:
        out = a.out / suite
        out.mkdir(exist_ok=True)
        fixtures = out / 'fixtures'
        shutil.copytree(a.cached_root / suite / source, fixtures, dirs_exist_ok=True)
        if a.compile:
            sources = sorted((a.cached_root / suite / 'pre').glob('*.i'))
            if not sources:
                raise RuntimeError(f'no cached preprocessed input for {suite}')
            for src in sources:
                run([a.keel, 'cc', *([] if a.plain else ['-O']), '--emit-object', src, '-o', out / (src.stem + '.o')])
        else:
            sources = sorted((a.cached_root / suite / ('build/keel' if a.plain else 'build/keel-O')).glob('*.s'))
            if not sources:
                raise RuntimeError(f'no cached Keel assembly for {suite}')
            for src in sources:
                run([a.keel, 'asm', src, '-o', out / (src.stem + '.o')])
        objects = sorted(out.glob('*.o'))
        native, apple = out / binary, out / (binary + '-apple')
        run([a.keel, 'link', *objects, '-lm', '-lpthread', '-o', native])
        run(['clang', *objects, '-lm', '-lpthread', '-o', apple])
        for exe in (native, apple):
            run(['codesign', '--verify', '--strict', exe])
            for tool, flags in [('otool', ['-l']), ('nm', ['-nm']), ('dyld_info', ['-imports', '-exports', '-fixups'])]:
                with (out / (exe.name + '.' + tool + '.txt')).open('w') as f:
                    run([tool, *flags, exe], stdout=f)
        if suite == 'lua':
            tests = [(['-e', '_U=true', 'all.lua'], fixtures)]
        elif suite == 'quickjs':
            tests = [(args + ['tests/' + name + '.js'], fixtures)
                     for args, name in [([], 'test_closure'), ([], 'test_language'),
                     ([], 'test_builtin'), ([], 'test_loop'), ([], 'test_std'),
                     ([], 'test_worker'), (['--bignum'], 'test_op_overloading'),
                     (['--bignum'], 'test_bignum'), (['--qjscalc'], 'test_qjscalc')]]
        else:
            tests = [([':memory:'], fixtures)]
        results = []
        for index, (args, cwd) in enumerate(tests):
            captures = []
            for exe in (native, apple):
                stdin = None
                if suite == 'sqlite':
                    stdin = (Path(__file__).parent / 'sqlite/queries.sql').read_bytes()
                result = run([exe, *args], cwd=cwd, input=stdin, capture_output=True)
                (out / f'{exe.name}.test-{index}.stdout').write_bytes(result.stdout)
                (out / f'{exe.name}.test-{index}.stderr').write_bytes(result.stderr)
                captures.append(result.stdout)
            if suite == 'sqlite' and captures[0] != captures[1]:
                raise RuntimeError('SQLite native output differs from Apple link of identical objects')
            results.append({'args': args, 'passed_native_and_apple': True})
        report[suite] = {'objects': len(objects), 'compiled': a.compile, 'optimized': not a.plain,
                         'native_size': native.stat().st_size, 'apple_size': apple.stat().st_size,
                         'tests': results}
        (a.out / 'results.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report, indent=2))

if __name__ == '__main__':
    main()
