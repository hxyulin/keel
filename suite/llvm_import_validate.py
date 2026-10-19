#!/usr/bin/env python3
"""Run through fast_opt_locked.py. Checks import guards and native equivalence.

llvm_import_validate.py --libllvm LIB --keel BIN --out NEW_ARTIFACT_DIR
"""
import argparse
import importlib.util
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
TOOL = ROOT / 'suite/llvm_import.py'
FIXTURES = ROOT / 'suite/llvm_import_fixtures'
HEADER = '''target datalayout = "e-m:o-i64:64-i128:128-n32:64-S128-Fn32"
target triple = "arm64-apple-macosx11.0.0"
'''


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--libllvm', required=True)
    p.add_argument('--keel', required=True)
    p.add_argument('--out', required=True)
    args = p.parse_args()
    out = Path(args.out).resolve()
    out.mkdir(parents=True, exist_ok=False)
    commands, checks = [], []

    def run(cmd):
        cmd = list(map(str, cmd))
        commands.append(cmd)
        result = subprocess.run(cmd, capture_output=True, timeout=90)
        assert result.returncode == 0, (cmd, result.returncode, result.stderr.decode(errors='replace'))
        return result.stdout

    def import_(path):
        dest = out / (path.stem + '.keel')
        run([sys.executable, TOOL, '--libllvm', args.libllvm, '--keel', args.keel, path, '-o', dest])
        return dest

    try:
        spec = importlib.util.spec_from_file_location('llvm_import', TOOL)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        llvm = module.LLVM(args.libllvm)
        cases = {
            'nsw': 'define i32 @f(i32 %a) { %v = add nsw i32 %a, 1\nret i32 %v }',
            'nuw': 'define i32 @f(i32 %a) { %v = mul nuw i32 %a, 2\nret i32 %v }',
            'exact': 'define i32 @f(i32 %a) { %v = udiv exact i32 %a, 2\nret i32 %v }',
            'disjoint': 'define i32 @f(i32 %a) { %v = or disjoint i32 %a, 1\nret i32 %v }',
            'nneg': 'define i32 @f(i8 %a) { %v = zext nneg i8 %a to i32\nret i32 %v }',
            'samesign': 'define i32 @f(i32 %a) { %b = icmp samesign slt i32 %a, 1\n%v = zext i1 %b to i32\nret i32 %v }',
            'poison': 'define i32 @f() { ret i32 poison }',
            'undef': 'define i32 @f() { ret i32 undef }',
            'freeze': 'define i32 @f() { %v = freeze i32 poison\nret i32 %v }',
            'alloca': 'define void @f() { %p = alloca i32\nret void }',
            'cfg': 'define i32 @f() { br label %b\nb: ret i32 1 }',
            'global': '@g = global i32 0\ndefine void @f() { ret void }',
            'atomic': 'define i32 @f(ptr %p) { %v = load atomic i32, ptr %p monotonic, align 4\nret i32 %v }',
            'abi-byval': 'define void @f(ptr byval(i32) %p) { ret void }',
            'abi-sret': 'define void @f(ptr sret(i32) %p) { ret void }',
            'variadic': 'declare i32 @f(i32, ...)',
            'bool-abi': 'define i1 @f() { ret i1 true }',
            'cc': 'define fastcc i32 @f() { ret i32 1 }',
            'bundle': 'declare void @g()\ndefine void @f() { call void @g() [ "deopt"(i32 1) ]\nret void }',
            'bad-call-type': 'declare i32 @g(i32)\ndefine i64 @f(i64 %x) { %v = call i64 @g(i64 %x)\nret i64 %v }',
            'bad-ir': 'this is not LLVM IR',
            'sme-definition': 'define void @f() "aarch64_pstate_sm_enabled" { ret void }',
            'sme-declaration': 'declare void @f() "aarch64_inout_za"',
            'sme-call': 'declare void @g()\ndefine void @f() { call void @g() "aarch64_pstate_sm_compatible"\nret void }',
            'future-arm-abi': 'define void @f() "aarch64_future_abi" { ret void }',
            'call-returns-twice': 'declare i32 @g(ptr)\ndefine i32 @f(ptr %p) { %v = call i32 @g(ptr %p) returns_twice\nret i32 %v }',
            'null-valid': 'define void @f() null_pointer_is_valid { ret void }',
            'instrumentation': 'define void @f() "instrument-function-entry"="hook" { ret void }',
            'patchable': 'define void @f() "patchable-function-entry"="4" { ret void }',
            'autolink': 'define void @f() { ret void }\n!llvm.linker.options = !{!0}\n!0 = !{!"-lmissing"}',
            'comdat': '$f = comdat any\ndefine i32 @f(i32 %x) comdat { ret i32 %x }',
            'plain-i8-return': 'define i8 @f(i8 %x) { ret i8 %x }',
            'plain-i16-return': 'define i16 @f(i16 %x) { ret i16 %x }',
            'noext-i8-return': 'define noext i8 @f(i8 %x) { ret i8 %x }',
            'noext-i16-return': 'define noext i16 @f(i16 %x) { ret i16 %x }',
        }
        for name, body in cases.items():
            text = HEADER + body + '\n'
            source = out / f'reject-{name}.ll'
            source.write_text(text)
            dest = out / 'preserved.keel'
            dest.write_text('sentinel\n')
            command = [sys.executable, str(TOOL), '--libllvm', args.libllvm, '--keel', args.keel, str(source), '-o', str(dest)]
            commands.append(command)
            result = subprocess.run(command, capture_output=True, timeout=30)
            assert result.returncode == 1, (name, result.returncode, result.stderr)
            assert not result.stdout and dest.read_text() == 'sentinel\n', name
            assert b'llvm-import:' in result.stderr, (name, result.stderr)
            checks.append(dict(kind='rejection', case=name, stderr=result.stderr.decode()))
        # Repeated invalid input also exercises LLVM error/module/context ownership.
        for _ in range(25):
            try:
                module.import_module(llvm, (HEADER + 'invalid').encode())
            except module.Unsupported:
                pass
            else:
                raise AssertionError('accepted invalid IR')

        cpp = out / 'cpp.ll'
        rust = out / 'rust.ll'
        run(['clang++', '-O2', '-fno-exceptions', '-fno-rtti', '-S', '-emit-llvm', FIXTURES/'scalar.cpp', '-o', cpp])
        run(['clang++', '-O2', '-fno-exceptions', '-fno-rtti', '-c', FIXTURES/'scalar.cpp', '-o', out/'cpp.o'])
        run(['rustc', '--edition', '2024', '--crate-type', 'lib', '-C', 'opt-level=2', '-C', 'panic=abort', '-C', 'overflow-checks=off', '--emit', f'llvm-ir={rust},obj={out}/rust.o', FIXTURES/'scalar.rs'])
        run(['clang', '-O2', '-c', FIXTURES/'driver.c', '-o', out/'driver.o'])
        for language in ['cpp', 'rust']:
            text = import_(out/f'{language}.ll')
            reference = out / f'{language}.reference'
            run(['clang', out/'driver.o', out/f'{language}.o', '-o', reference])
            expected = run([reference])
            for tier in ['none', 'fast', 'full']:
                obj = out / f'{language}.{tier}.o'
                exe = out / f'{language}.{tier}'
                run([args.keel, 'compile', '--opt-tier', tier, '--emit-object', text, '-o', obj])
                run(['clang', out/'driver.o', obj, '-o', exe])
                actual = run([exe])
                assert actual == expected, (language, tier)
                checks.append(dict(kind='native', language=language, tier=tier, pairs=20289, exports=8, checksum=actual.decode().strip()))

        text = import_(FIXTURES/'boolean.ll')
        run(['clang', '-O2', '-c', FIXTURES/'boolean_driver.c', '-o', out/'boolean-driver.o'])
        run(['clang', '-c', FIXTURES/'boolean.ll', '-o', out/'boolean-reference.o'])
        run(['clang', out/'boolean-driver.o', out/'boolean-reference.o', '-o', out/'boolean.reference'])
        run([out/'boolean.reference'])
        for tier in ['none', 'fast', 'full']:
            obj = out/f'boolean.{tier}.o'
            exe = out/f'boolean.{tier}'
            run([args.keel, 'compile', '--opt-tier', tier, '--emit-object', text, '-o', obj])
            run(['clang', out/'boolean-driver.o', obj, '-o', exe])
            run([exe])
            checks.append(dict(kind='native', language='handwritten-i1', tier=tier, inputs=65536, exports=3))
        print(json.dumps({'checks':len(checks), 'native_builds':9, 'rejection_cases':len(cases)}))
    finally:
        (out/'commands.json').write_text(json.dumps(commands, indent=2))
        (out/'checks.json').write_text(json.dumps(checks, indent=2))


if __name__ == '__main__':
    main()
