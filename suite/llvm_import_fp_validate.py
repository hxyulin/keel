#!/usr/bin/env python3
"""Scalar FP importer checks; invoke through suite/fast_opt_locked.py.

No lock is taken here, so an aggregate runner may hold the host lease itself.
All compiler output is consumed unchanged. --out must be a new private directory.
FP->int comparisons use only LLVM-defined inputs (truncated value in range).
Arithmetic NaNs compare by classification; bitcasts/constants/fneg compare bits.
"""
import argparse
import hashlib
import json
import re
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
TOOL = ROOT / 'suite/llvm_import.py'
FIXTURES = ROOT / 'suite/llvm_import_fp_fixtures'
HEADER = '''target datalayout = "e-m:o-i64:64-i128:128-n32:64-S128-Fn32"
target triple = "arm64-apple-macosx11.0.0"
'''


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--libllvm', required=True)
    p.add_argument('--keel', required=True)
    p.add_argument('--clang', default='clang')
    p.add_argument('--out', required=True)
    args = p.parse_args()
    out = Path(args.out).resolve()
    out.mkdir(parents=True, exist_ok=False)
    commands, checks = [], []

    def run(command, ok=True):
        command = list(map(str, command))
        commands.append(command)
        result = subprocess.run(command, capture_output=True, timeout=120)
        if ok:
            assert result.returncode == 0, (command, result.returncode, result.stderr.decode(errors='replace'))
        return result

    def importer(source, dest, keel=None):
        return run([sys.executable, TOOL, '--libllvm', args.libllvm,
                    '--keel', keel or args.keel, source, '-o', dest], ok=False)

    try:
        cases = {
            'frem': 'define double @f(double %x) { %r = frem double %x, 2.0\nret double %r }',
            'half': 'define half @f(half %x) { ret half %x }',
            'bfloat': 'define bfloat @f(bfloat %x) { ret bfloat %x }',
            'x87': 'define x86_fp80 @f(x86_fp80 %x) { ret x86_fp80 %x }',
            'fp128': 'define fp128 @f(fp128 %x) { ret fp128 %x }',
            'vector': 'define <2 x float> @f(<2 x float> %x) { ret <2 x float> %x }',
            'vector-layout': 'define void @f() { %p = alloca <2 x float>\nret void }',
            'strict': 'define double @f(double %x) strictfp { ret double %x }',
            'strict-decl': 'declare double @f(double) strictfp',
            'strict-call': 'declare double @g(double)\ndefine double @f(double %x) { %v = call double @g(double %x) strictfp\nret double %v }',
            'constrained': 'declare double @llvm.experimental.constrained.fadd.f64(double, double, metadata, metadata)\ndefine double @f(double %x) { %v = call double @llvm.experimental.constrained.fadd.f64(double %x, double %x, metadata !"round.downward", metadata !"fpexcept.strict")\nret double %v }',
            'nneg': 'define double @f(i32 %x) { %v = uitofp nneg i32 %x to double\nret double %v }',
            'poison': 'define double @f() { ret double poison }',
            'undef': 'define float @f() { ret float undef }',
            'freeze': 'define double @f(double %x) { %v = freeze double %x\nret double %v }',
            'indirect-nofpclass': 'define double @f(ptr %p, double %x) { %v = call nofpclass(nan) double %p(double %x)\nret double %v }',
            'half-global': '@g = global half 1.0\ndefine double @f() { ret double 1.0 }',
            'nofpclass-return': 'define nofpclass(nan) double @f(double %x) { ret double %x }',
            'nofpclass-param': 'declare double @f(double nofpclass(inf))',
            'nofpclass-call': 'declare double @g(double)\ndefine double @f(double %x) { %v = call nofpclass(nan) double @g(double %x)\nret double %v }',
            'nofpclass-call-param': 'declare double @g(double)\ndefine double @f(double %x) { %v = call double @g(double nofpclass(nan) %x)\nret double %v }',
            'late-failure': 'define double @good() { ret double 1.0 }\ndefine double @bad() { %p = fadd fast double 1.0, 2.0\nret double %p }',
            # Unsupported ABI contracts stay rejected.
            'plain-i8-return': 'define i8 @f(i8 %x) { ret i8 %x }',
            'extension-mismatch': 'declare signext i8 @g(i8)\ndefine signext i8 @f(i8 %x) { %v = call i8 @g(i8 %x)\nret i8 %v }',
            'byval': 'declare double @g(ptr byval(double))',
            'sret': 'declare void @g(ptr sret(double))',
        }
        for flag in ['fast', 'nnan', 'ninf', 'nsz', 'arcp', 'contract', 'afn', 'reassoc']:
            cases['flag-' + flag] = f'define double @f(double %x) {{ %v = fadd {flag} double %x, 1.0\nret double %v }}'
        for op,body in [('fcmp', '%v = fcmp nnan olt double %x, 1.0\n%z = zext i1 %v to i32\nret i32 %z'),
                        ('fneg', '%v = fneg nsz double %x\nret double %v'),
                        ('select', '%v = select nnan i1 true, double %x, double 0.0\nret double %v'),
                        ('call', '%v = call nnan double @g(double %x)\nret double %v')]:
            ret = 'i32' if op == 'fcmp' else 'double'
            cases['flag-' + op] = 'declare double @g(double)\n' + f'define {ret} @f(double %x) {{ {body} }}'
        for width in [1, 8, 16]:
            for op in ['fptosi', 'fptoui']:
                cases[f'{op}-i{width}'] = f'define void @f(double %x) {{ %v = {op} double %x to i{width}\nret void }}'
        cases['flag-fptrunc'] = 'define float @f(double %x) { %v = fptrunc nnan double %x to float\nret float %v }'
        for attr in ['denormal-fp-math', 'denormal-fp-math-f32', 'rounding-mode',
                     'unsafe-fp-math', 'no-infs-fp-math', 'no-nans-fp-math',
                     'no-signed-zeros-fp-math', 'approx-func-fp-math']:
            cases['attribute-' + attr] = f'define double @f(double %x) "{attr}"="true" {{ ret double %x }}'
        for name, body in cases.items():
            source = out / f'reject-{name}.ll'
            original = (HEADER + body + '\n').encode()
            source.write_bytes(original)
            dest = out / 'preserved.keel'
            dest.write_bytes(b'sentinel\n')
            result = importer(source, dest)
            assert result.returncode == 1 and not result.stdout, (name, result)
            assert b'llvm-import:' in result.stderr, (name, result.stderr)
            assert source.read_bytes() == original and dest.read_bytes() == b'sentinel\n', name
            # A new destination must not appear on failure either.
            absent = out / 'absent.keel'
            result2 = importer(source, absent)
            assert result2.returncode == 1 and not absent.exists(), name
            checks.append(dict(kind='rejection-preservation', case=name, diagnostic=result.stderr.decode()))

        compiler = out / 'compiler.ll'
        run([args.clang, '-O1', '-ffp-contract=off', '-S', '-emit-llvm', FIXTURES/'compiler.c', '-o', compiler])
        compiler_o0 = out / 'compiler-o0.ll'
        run([args.clang, '-O0', '-ffp-contract=off', '-S', '-emit-llvm', FIXTURES/'compiler.c', '-o', compiler_o0])
        for name, source, driver in [('scalar', FIXTURES/'scalar.ll', FIXTURES/'driver.c'),
                                     ('compiler', compiler, FIXTURES/'compiler_driver.c'),
                                     ('compiler-o0', compiler_o0, FIXTURES/'compiler_driver.c')]:
            original = source.read_bytes()
            text = out / f'{name}.keel'
            text.write_text('replace-on-success\n')
            result = importer(source, text)
            assert result.returncode == 0, result.stderr.decode()
            assert source.read_bytes() == original and text.read_text() != 'replace-on-success\n'
            # Every definition/declaration survives, including unused helpers.
            source_names = re.findall(rb'^(?:define|declare)\b[^@\n]*@([\w.$]+)\(', original, re.M)
            output_names = re.findall(r'^(?:func|decl) @([\w.$]+)\(', text.read_text(), re.M)
            assert [name.decode() for name in source_names] == output_names
            checks.append(dict(kind='accepted-source-preservation', case=name,
                               sha256=hashlib.sha256(original).hexdigest()))
            if name == 'scalar':
                imported = text.read_text()
                for constant in ['0x7ff0000000000001', '0x8000000000000000', '0x0000000000000001', '0x7f800001', '0x00000001']:
                    assert constant in imported, constant
            # Compile the same immutable LLVM input, not a fresh C compilation.
            run([args.clang, '-O0', '-c', source, '-o', out/f'{name}.reference.o'])
            run([args.clang, '-O0', '-ffp-contract=off', '-c', driver, '-o', out/f'{name}.driver.o'])
            run([args.clang, out/f'{name}.driver.o', out/f'{name}.reference.o', '-o', out/f'{name}.reference'])
            expected = run([out/f'{name}.reference']).stdout
            for tier in ['none', 'fast', 'full']:
                obj, exe = out/f'{name}.{tier}.o', out/f'{name}.{tier}'
                run([args.keel, 'compile', '--opt-tier', tier, '--emit-object', text, '-o', obj])
                run([args.clang, out/f'{name}.driver.o', obj, '-o', exe])
                actual = run([exe]).stdout
                assert actual == expected, (name, tier)
                checks.append(dict(kind='native-independent-assertions', case=name, tier=tier))
            assert source.read_bytes() == original
        dest = out/'verifier-preserved.keel'
        dest.write_bytes(b'sentinel\n')
        original = (FIXTURES/'scalar.ll').read_bytes()
        result = importer(FIXTURES/'scalar.ll', dest, '/usr/bin/false')
        assert result.returncode == 1 and not result.stdout and b'Keel verification failed' in result.stderr
        assert dest.read_bytes() == b'sentinel\n' and (FIXTURES/'scalar.ll').read_bytes() == original
        checks.append(dict(kind='downstream-verifier-preservation'))
        print(json.dumps(dict(rejections=len(cases), native_builds=9, checks=len(checks))))
    finally:
        (out/'commands.json').write_text(json.dumps(commands, indent=2))
        (out/'checks.json').write_text(json.dumps(checks, indent=2))


if __name__ == '__main__':
    main()
