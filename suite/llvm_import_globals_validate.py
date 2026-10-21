#!/usr/bin/env python3
"""Globals guards/native assertions. Run under suite/fast_opt_locked.py (no inner lock).

--libllvm LLVM23_DYLIB --keel PRIVATE_BIN --out NEW_PRIVATE_ARTIFACT_DIR
Compiler .ll outputs are consumed verbatim and checked for byte preservation.
"""
import argparse
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
TOOL = ROOT / 'suite/llvm_import.py'
FIXTURES = ROOT / 'suite/llvm_import_globals_fixtures'
HEADER = ('target datalayout = "e-m:o-i64:64-i128:128-n32:64-S128-Fn32"\n'
          'target triple = "arm64-apple-macosx11.0.0"\n')


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--libllvm', required=True)
    p.add_argument('--keel', required=True)
    p.add_argument('--out', required=True)
    args = p.parse_args()
    out = Path(args.out).resolve()
    out.mkdir(parents=True, exist_ok=False)
    commands, checks = [], []

    def run(cmd, ok=True):
        cmd = list(map(str, cmd))
        commands.append(cmd)
        result = subprocess.run(cmd, capture_output=True, timeout=90)
        assert (result.returncode == 0) == ok, (cmd, result.returncode, result.stderr.decode(errors='replace'))
        return result

    def import_command(source, dest):
        return [sys.executable, TOOL, '--libllvm', args.libllvm, '--keel', args.keel, source, '-o', dest]

    try:
        cases = json.loads((FIXTURES/'rejections.json').read_text())
        for name, body in cases.items():
            source = out / f'reject-{name}.ll'
            original = (HEADER + body + '\n').encode()
            source.write_bytes(original)
            dest = out / 'preserved.keel'
            dest.write_bytes(b'sentinel\n')
            result = run(import_command(source, dest), ok=False)
            assert result.returncode == 1 and b'llvm-import:' in result.stderr, (name, result.stderr)
            # These are valid LLVM modules; exercise importer guards, not parse errors.
            assert b'input.ll:' not in result.stderr and b'input verification' not in result.stderr, (name, result.stderr)
            assert not result.stdout and dest.read_bytes() == b'sentinel\n', name
            assert source.read_bytes() == original, name
            missing = out / 'missing.keel'
            result2 = run(import_command(source, missing), ok=False)
            assert not missing.exists() and not result2.stdout and source.read_bytes() == original, name
            checks.append(dict(kind='rejection', case=name, stderr=result.stderr.decode()))
        for name, compiler, source, driver in [
            ('clang-c', 'clang', FIXTURES/'objects.c', FIXTURES/'driver.c'),
            ('clang-cpp', 'clang++', FIXTURES/'objects.cpp', FIXTURES/'driver.c'),
            ('layout', 'clang', FIXTURES/'layout.ll', FIXTURES/'layout_driver.c'),
        ]:
            if name != 'layout':
                emitted = out / f'{name}.ll'
                run([compiler, '-O0', '-fno-common', '-S', '-emit-llvm', source, '-o', emitted])
                source = emitted
            original = source.read_bytes()
            dest = out / f'{name}.keel'
            dest.write_text('sentinel\n')
            # Even a failure in the final Keel verifier must preserve output.
            failing = import_command(source, dest)
            failing[failing.index('--keel') + 1] = '/usr/bin/false'
            rejected = run(failing, ok=False)
            assert b'Keel verification failed' in rejected.stderr and not rejected.stdout
            assert dest.read_bytes() == b'sentinel\n' and source.read_bytes() == original
            run(import_command(source, dest))
            assert source.read_bytes() == original and dest.read_text() != 'sentinel\n'
            # Import via stdout must publish the same complete module.
            assert run(import_command(source, '-')).stdout == dest.read_bytes()
            run(['clang', '-c', source, '-o', out/f'{name}.reference.o'])
            run(['clang', '-O0', '-c', driver, '-o', out/f'{name}.driver.o'])
            run(['clang', out/f'{name}.driver.o', out/f'{name}.reference.o', '-o', out/f'{name}.reference'])
            expected = run([out/f'{name}.reference']).stdout
            assert expected == (b'nested layout assertions passed\n' if name == 'layout' else b'global object assertions passed\n')
            for tier in ('none', 'fast', 'full'):
                obj, exe = out/f'{name}.{tier}.o', out/f'{name}.{tier}'
                run([args.keel, 'compile', '--opt-tier', tier, '--emit-object', dest, '-o', obj])
                run(['clang', out/f'{name}.driver.o', obj, '-o', exe])
                assert run([exe]).stdout == expected
                assert source.read_bytes() == original
                checks.append(dict(kind='native-assertions', case=name, tier=tier))
    finally:
        (out/'results.json').write_text(json.dumps(dict(commands=commands, checks=checks), indent=2) + '\n')
    print(f'{len(checks)} globals checks passed')


if __name__ == '__main__':
    main()
