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
            'dynamic-alloca': 'define void @f(i64 %n) { %p = alloca i32, i64 %n\nret void }',
            'loop-alloca': 'define void @f() { br label %b\nb: %p = alloca i32\nret void }',
            'zero-alloca': 'define void @f() { %p = alloca i32, i64 0\nret void }',
            'large-alloca': 'define void @f() { %p = alloca i64, i64 536870912\nret void }',
            'aligned-alloca': 'define void @f() { %p = alloca i32, align 32\nret void }',
            'opaque-alloca': '%S = type opaque\ndefine void @f() { %p = alloca %S\nret void }',
            'half-alloca': 'define void @f() { %p = alloca half\nret void }',
            'address-space': 'define ptr addrspace(1) @f(ptr addrspace(1) %p) { ret ptr addrspace(1) %p }',
            'gep-wide-index': 'define ptr @f(ptr %p, i128 %i) { %q = getelementptr i64, ptr %p, i128 %i\nret ptr %q }',
            'gep-i1': 'define ptr @f(ptr %p) { %q = getelementptr i8, ptr %p, i1 true\nret ptr %q }',
            'phi-undef': 'define i32 @f(i32 %x) { br label %b\nb: %p = phi i32 [ undef, %0 ]\nret i32 %p }',
            'bad-phi': 'define i32 @f(i32 %x) { br label %b\nb: %p = phi i32 [ %x, %b ]\nret i32 %p }',
            'duplicate-phi': 'define i32 @f(i32 %x) { %c = icmp eq i32 %x, 0\nbr i1 %c, label %b, label %b\nb: %p = phi i32 [ 1, %0 ], [ 2, %0 ]\nret i32 %p }',
            'late-failure': 'define i32 @good() { ret i32 7 }\ndefine i32 @bad() { ret i32 poison }',
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
        headers = {
            'wrong-target': HEADER.replace('arm64-apple-macosx11.0.0', 'x86_64-unknown-linux-gnu'),
            'big-endian': HEADER.replace('e-m:o', 'E-m:o'),
            'pointer-index32': HEADER.replace('e-m:o', 'e-m:o-p:64:64:64:32'),
            'missing-layout': 'target triple = "arm64-apple-macosx11.0.0"\n',
            'missing-target': HEADER.split('target triple')[0],
        }
        cases.update({name: 'define i32 @f() { ret i32 1 }' for name in headers})
        for name, body in cases.items():
            text = headers.get(name, HEADER) + body + '\n'
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
        # Handwritten hard CFG/PHI cases plus untouched compiler-emitted CFG/GEP.
        run(['clang', '-O1', '-S', '-emit-llvm', FIXTURES/'cfg.c', '-o', out/'cfg-clang.ll'])
        run(['clang', '-O1', '-c', FIXTURES/'cfg.c', '-o', out/'cfg-clang.reference.o'])
        run(['rustc', '--edition', '2024', '--crate-type', 'lib', '-C', 'opt-level=1', '-C', 'panic=abort', '--emit', f'llvm-ir={out}/cfg-rust.ll,obj={out}/cfg-rust.reference.o', FIXTURES/'cfg.rs'])
        run(['clang', '-c', FIXTURES/'cfg_memory.ll', '-o', out/'cfg-memory.reference.o'])
        for name, source, driver, inputs in [
            ('cfg-memory', FIXTURES/'cfg_memory.ll', 'cfg_memory_driver.c', 4096),
            ('cfg-clang', out/'cfg-clang.ll', 'cfg_compiler_driver.c', 8192),
            ('cfg-rust', out/'cfg-rust.ll', 'cfg_compiler_driver.c', 8192),
        ]:
            text = import_(source)
            run(['clang', '-O2', '-c', FIXTURES/driver, '-o', out/f'{name}.driver.o'])
            run(['clang', out/f'{name}.driver.o', out/f'{name}.reference.o', '-o', out/f'{name}.reference'])
            expected = run([out/f'{name}.reference'])
            for tier in ['none', 'fast', 'full']:
                obj, exe = out/f'{name}.{tier}.o', out/f'{name}.{tier}'
                run([args.keel, 'compile', '--opt-tier', tier, '--emit-object', text, '-o', obj])
                run(['clang', out/f'{name}.driver.o', obj, '-o', exe])
                actual = run([exe])
                assert actual == expected, (name, tier)
                checks.append(dict(kind='native', language=name, tier=tier, inputs=inputs, checksum=actual.decode().strip()))
        # A failing downstream verifier must also leave the destination untouched.
        dest = out/'verifier-preserved.keel'
        dest.write_text('sentinel\n')
        cmd = [sys.executable, str(TOOL), '--libllvm', args.libllvm, '--keel', '/usr/bin/false', str(FIXTURES/'cfg_memory.ll'), '-o', str(dest)]
        commands.append(cmd)
        result = subprocess.run(cmd, capture_output=True, timeout=30)
        assert result.returncode == 1 and not result.stdout and dest.read_text() == 'sentinel\n'
        assert b'Keel verification failed' in result.stderr
        checks.append(dict(kind='output-preservation', case='downstream-verifier-failure'))
        print(json.dumps({'checks':len(checks), 'native_builds':18, 'rejection_cases':len(cases)}))
    finally:
        (out/'commands.json').write_text(json.dumps(commands, indent=2))
        (out/'checks.json').write_text(json.dumps(checks, indent=2))


if __name__ == '__main__':
    main()
