#!/usr/bin/env python3
"""Standalone call/ABI checks; run under fast_opt_locked.py (no internal lock).

--libllvm LIB --keel BIN --out NEW_ARTIFACT_DIR [--clang clang]
Compiler-emitted LLVM is imported byte-for-byte; no IR sanitization is applied.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
TOOL = ROOT / 'suite/ir/llvm_import.py'
FIXTURES = ROOT / 'suite/ir/llvm_import_calls_fixtures'
HEADER = (FIXTURES / 'phi.ll').read_text().split('declare')[0]


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
    report = dict(status='failed', checks=checks, commands=commands,
                  aggregate_contract='first ptr sret of unpacked {i64,i64,i64}, void result, size 24, align 8, offsets 0/8/16; x8 transport',
                  deferred_aggregates={
                      'larger_integer_layouts': 'Recursively compare LLVM and Keel size, alignment and offsets; prove non-HFA indirect classification before accepting each form.',
                      'small_and_hfa_sret': 'LLVM sret already requests x8; Keel reclassifies to registers. Needs explicit lowered ABI transport support, not copied attributes.',
                      'byval': 'LLVM parameter already carries a lowered ABI contract; prove caller copy/alias and physical argument equivalence before any Keel source-layout reclassification.',
                      'aggregate_ssa': 'Represent internal aggregate values and extractvalue/insertvalue without exposing source aggregates as lowered call signatures; test poison/undef and nested layout separately.'})

    def run(cmd, success=True):
        cmd = list(map(str, cmd))
        result = subprocess.run(cmd, capture_output=True, timeout=120)
        commands.append(dict(argv=cmd, returncode=result.returncode,
                             stderr=result.stderr.decode(errors='replace')))
        if success:
            assert result.returncode == 0, commands[-1]
        return result

    def import_(source, dest, success=True):
        before = source.read_bytes()
        result = run([sys.executable, TOOL, '--libllvm', args.libllvm,
                      '--keel', args.keel, source, '-o', dest], success)
        assert source.read_bytes() == before, source
        return result

    try:
        flags = ['-O1', '-fwrapv', '-Xclang', '-disable-lifetime-markers']
        sources = {}
        for name in ('calls', 'aggregate'):
            source = out / f'{name}.ll'
            run([args.clang, *flags, '-S', '-emit-llvm', FIXTURES/f'{name}.c', '-o', source])
            sources[name] = source
        sources['phi'] = FIXTURES/'phi.ll'
        run([args.clang, '-O2', '-c', FIXTURES/'driver.c', '-o', out/'driver.o'])
        imported = {}
        for name, source in sources.items():
            dest = out/f'{name}.keel'
            dest.write_text('replace only after verification\n')
            import_(source, dest)
            imported[name] = dest
            run([args.clang, '-c', source, '-o', out/f'{name}.reference.o'])
            checks.append(dict(kind='source-preservation', case=name,
                               sha256=hashlib.sha256(source.read_bytes()).hexdigest()))
        assert 'call_indirect' in imported['calls'].read_text()
        assert 'symbol_addr @external_add' in imported['calls'].read_text()
        assert 'ptr sret {i64, i64, i64}' in imported['aggregate'].read_text()
        run([args.clang, out/'driver.o', *[out/f'{n}.reference.o' for n in sources], '-o', out/'reference'])
        expected = run([out/'reference']).stdout
        assert b'2049 independently asserted inputs' in expected
        # Replace one compiler-produced object at a time AND all objects together.
        # This tests native caller -> Keel and Keel -> native callback/aggregate.
        for tier in ('none', 'fast', 'full'):
            for name, source in imported.items():
                run([args.keel, 'compile', '--opt-tier', tier, '--emit-object', source,
                     '-o', out/f'{name}.{tier}.o'])
            for replacement in ('calls', 'aggregate', 'phi', 'all'):
                objects = [out/f'{n}.{tier if replacement in (n, "all") else "reference"}.o' for n in sources]
                exe = out/f'{replacement}.{tier}'
                run([args.clang, out/'driver.o', *objects, '-o', exe])
                assert run([exe]).stdout == expected, (tier, replacement)
                checks.append(dict(kind='native', tier=tier, replacement=replacement, inputs=2049,
                                   independent_assertions=True))

        negative = {
            'cc': 'define i32 @bad(ptr %f) { %r = call fastcc i32 %f()\nret i32 %r }',
            'varargs': 'define i32 @bad(ptr %f) { %r = call i32 (...) %f(i32 1)\nret i32 %r }',
            'musttail': 'define i32 @bad(ptr %f) { %r = musttail call i32 %f(ptr %f)\nret i32 %r }',
            'bundle': 'define void @bad(ptr %f) { call void %f() [ "deopt"(i32 1) ]\nret void }',
            'byval': 'define void @bad(ptr %f, ptr %p) { call void %f(ptr byval(i64) %p)\nret void }',
            'small-sret': 'define void @bad(ptr %f, ptr %p) { call void %f(ptr sret({i64,i64}) %p)\nret void }',
            'hfa-sret': 'define void @bad(ptr %f, ptr %p) { call void %f(ptr sret({double,double,double}) %p)\nret void }',
            'packed-sret': 'define void @bad(ptr %f, ptr %p) { call void %f(ptr sret(<{i64,i64,i64}>) %p)\nret void }',
            'other-sret': 'define void @bad(ptr %f, ptr %p) { call void %f(ptr sret({i64,i64,i64,i64}) %p)\nret void }',
            'misplaced-sret': 'define void @bad(ptr %f, ptr %p) { call void %f(i64 1, ptr sret({i64,i64,i64}) %p)\nret void }',
            'nonvoid-sret': 'define i64 @bad(ptr %f, ptr %p) { %r = call i64 %f(ptr sret({i64,i64,i64}) %p)\nret i64 %r }',
            'inreg': 'define i32 @bad(ptr %f) { %r = call i32 %f(i32 inreg 1)\nret i32 %r }',
            'noext': 'define signext i8 @bad(ptr %f) { %r = call noext i8 %f()\nret i8 %r }',
            'missing-extension': 'define i32 @bad(ptr %f) { %r = call i8 %f()\n%x = zext i8 %r to i32\nret i32 %x }',
            'direct-extension': 'declare signext i8 @g(i8 signext)\ndefine signext i8 @bad(i8 signext %x) { %r = call zeroext i8 @g(i8 signext %x)\nret i8 %r }',
            'direct-parameter-extension': 'declare i32 @g(i8 signext)\ndefine i32 @bad(i8 signext %x) { %r = call i32 @g(i8 zeroext %x)\nret i32 %r }',
            'direct-prototype': 'declare i32 @g(i32)\ndefine i64 @bad(i64 %x) { %r = call i64 @g(i64 %x)\nret i64 %r }',
            'direct-sret-mismatch': 'declare void @g(ptr sret({i64,i64,i64}))\ndefine void @bad(ptr %p) { call void @g(ptr %p)\nret void }',
            'unknown-string-contract': 'define i32 @bad(ptr %f) { %r = call i32 %f(i32 "future-abi" 1)\nret i32 %r }',
            'unknown-call-contract': 'define i32 @bad(ptr %f) { %r = call i32 %f() \"future-call-abi\"\nret i32 %r }',
            'returns-twice': 'define i32 @bad(ptr %f) { %r = call i32 %f() returns_twice\nret i32 %r }',
            'aggregate-ssa': 'define {i64,i64} @bad() { ret {i64,i64} zeroinitializer }',
        }
        for name, body in negative.items():
            source, dest = out/f'reject-{name}.ll', out/'preserved.keel'
            # A good function before the rejected function checks whole-module buffering.
            source.write_text(HEADER + 'define i32 @good() { ret i32 7 }\n' + body + '\n')
            dest.write_text('sentinel\n')
            result = import_(source, dest, False)
            assert result.returncode == 1 and not result.stdout, (name, result)
            assert dest.read_text() == 'sentinel\n', name
            assert b'llvm-import:' in result.stderr
            # Rejecting to stdout must not expose a partial module either.
            result = run([sys.executable, TOOL, '--libllvm', args.libllvm, '--keel', args.keel, source], False)
            assert result.returncode == 1 and not result.stdout, name
            absent = out/f'absent-{name}.keel'
            result = import_(source, absent, False)
            assert result.returncode == 1 and not result.stdout and not absent.exists(), name
            checks.append(dict(kind='rejection-preservation', case=name,
                               stderr=result.stderr.decode(errors='replace')))
        dest = out/'preserved.keel'
        dest.write_text('sentinel\n')
        before = sources['calls'].read_bytes()
        result = run([sys.executable, TOOL, '--libllvm', args.libllvm, '--keel', '/usr/bin/false',
                      sources['calls'], '-o', dest], False)
        assert result.returncode == 1 and not result.stdout
        assert dest.read_text() == 'sentinel\n' and sources['calls'].read_bytes() == before
        assert b'Keel verification failed' in result.stderr
        checks.append(dict(kind='output-preservation', case='downstream-verifier'))
        report['status'] = 'passed'
        print(json.dumps(dict(status='passed', native_runs=13, negative_cases=len(negative), checks=len(checks))))
    finally:
        (out/'validation.json').write_text(json.dumps(report, indent=2) + '\n')
        (out/'checks.json').write_text(json.dumps(checks, indent=2) + '\n')
        (out/'commands.json').write_text(json.dumps(commands, indent=2) + '\n')


if __name__ == '__main__':
    main()
