#!/usr/bin/env python3
"""Bounded Itanium EH checks; run under fast_opt_locked.py (no internal lock).

Uses actual, unmodified Clang++ IR and external native throwing functions.
Requires Apple AArch64, LLVM 23, and a Keel executable. Private output directory
must not exist. All native executions have independent assertions enabled.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
TOOL = ROOT / 'suite/llvm_import.py'
FIXTURES = ROOT / 'suite/llvm_import_eh_fixtures'
HEADER = (FIXTURES / 'edges.ll').read_text().split('declare')[0]
PREAMBLE = '''declare i32 @__gxx_personality_v0(...)
declare i32 @eh_throw(i32)
'''
PAD = '''define i32 @bad(i32 %x) personality ptr @__gxx_personality_v0 {
entry:
 %r = invoke i32 @eh_throw(i32 %x) to label %normal unwind label %pad
normal:
 ret i32 %r
pad:
 %lp = landingpad {ptr, i32} cleanup
 resume {ptr, i32} %lp
}
'''


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--libllvm', required=True)
    p.add_argument('--keel', required=True)
    p.add_argument('--clang', default='clang++')
    p.add_argument('--out', required=True)
    args = p.parse_args()
    out = Path(args.out).resolve()
    out.mkdir(parents=True, exist_ok=False)
    checks, commands = [], []
    report = dict(status='failed', checks=checks, commands=commands)

    def run(cmd, success=True):
        cmd = list(map(str, cmd))
        r = subprocess.run(cmd, capture_output=True, timeout=120)
        commands.append(dict(argv=cmd, returncode=r.returncode,
                             stderr=r.stderr.decode(errors='replace')))
        if success:
            assert r.returncode == 0, commands[-1]
        return r

    def import_(source, dest=None, success=True, keel=None, stdin=False):
        before = source.read_bytes()
        cmd = [sys.executable, TOOL, '--libllvm', args.libllvm,
               '--keel', keel or args.keel, '-' if stdin else source]
        if dest is not None:
            cmd += ['-o', dest]
        if stdin:
            r = subprocess.run(list(map(str, cmd)), input=before, capture_output=True, timeout=120)
            commands.append(dict(argv=list(map(str, cmd)), returncode=r.returncode,
                                 stderr=r.stderr.decode(errors='replace')))
            if success:
                assert r.returncode == 0, commands[-1]
        else:
            r = run(cmd, success)
        assert source.read_bytes() == before
        return r

    try:
        run([args.clang, '--version'])
        source = out/'catches.ll'
        run([args.clang, '-O1', '-fwrapv', '-Xclang', '-disable-lifetime-markers',
             '-S', '-emit-llvm', FIXTURES/'catches.cpp', '-o', source])
        sources = dict(catches=source, edges=FIXTURES/'edges.ll')
        run([args.clang, '-O2', '-c', FIXTURES/'driver.cpp', '-o', out/'driver.o'])
        for name, path in sources.items():
            dest = out/f'{name}.keel'
            dest.write_text('sentinel\n')
            import_(path, dest)
            assert import_(path, stdin=True).stdout == dest.read_bytes()
            run([args.clang, '-O1', '-c', path, '-o', out/f'{name}.reference.o'])
            checks.append(dict(kind='source-preservation', case=name,
                               sha256=hashlib.sha256(path.read_bytes()).hexdigest()))
        snapshot = FIXTURES/'catches.clang.ll'
        import_(snapshot, out/'snapshot.keel')
        checks.append(dict(kind='source-preservation', case='clang-snapshot',
                           sha256=hashlib.sha256(snapshot.read_bytes()).hexdigest()))
        text = (out/'catches.keel').read_text()
        assert all(f'func @{name}(' in text for name in ('eh_catches', 'eh_typed', 'eh_cleaned', 'eh_rethrow'))
        assert 'landingpad catch @_ZTIi, catch null' in text
        assert 'eh_typeid @_ZTIi' in text and 'call @_Unwind_Resume(' in text
        assert 'invoke @__cxa_rethrow(' in text
        edges = (out/'edges.keel').read_text()
        assert edges.count('landingpad catch null') == 2
        run([args.clang, out/'driver.o', *[out/f'{n}.reference.o' for n in sources], '-o', out/'reference'])
        expected = run([out/'reference']).stdout
        assert expected == b'EH: 1130 independently asserted inputs\n'
        for tier in ('none', 'fast', 'full'):
            for name in sources:
                run([args.keel, 'compile', '--opt-tier', tier, '--emit-object',
                     out/f'{name}.keel', '-o', out/f'{name}.{tier}.o'])
            run([args.keel, 'compile', '--opt-tier', tier, '--emit-object',
                 out/'snapshot.keel', '-o', out/f'snapshot.{tier}.o'])
            exe = out/f'snapshot.{tier}'
            run([args.clang, out/'driver.o', out/f'snapshot.{tier}.o', out/'edges.reference.o', '-o', exe])
            assert run([exe]).stdout == expected
            checks.append(dict(kind='native', tier=tier, replacement='clang-snapshot',
                               inputs=1130, independent_assertions=True))
            for replacement in ('catches', 'edges', 'all'):
                objects = [out/f'{n}.{tier if replacement in (n, "all") else "reference"}.o' for n in sources]
                exe = out/f'{replacement}.{tier}'
                run([args.clang, out/'driver.o', *objects, '-o', exe])
                assert run([exe]).stdout == expected, (tier, replacement)
                checks.append(dict(kind='native', tier=tier, replacement=replacement,
                                   inputs=1130, independent_assertions=True))

        negative = {
            'indirect-invoke': PREAMBLE + PAD.replace('@bad(i32 %x)', '@bad(i32 %x, ptr %f)').replace('invoke i32 @eh_throw', 'invoke i32 %f'),
            'filter': PREAMBLE + PAD.replace('cleanup\n', 'filter [1 x ptr] [ptr null]\n'),
            'personality-name': (PREAMBLE + PAD).replace('__gxx_personality_v0', 'other_personality'),
            'personality-prototype': (PREAMBLE + PAD).replace('@__gxx_personality_v0(...)', '@__gxx_personality_v0(i32, ...)'),
            'personality-address': PREAMBLE + '@addr = global ptr @__gxx_personality_v0\n' + PAD,
            'personality-call': PREAMBLE + PAD.replace('ret i32 %r', '%p = call i32 (...) @__gxx_personality_v0()\n ret i32 %r'),
            'invoke-cc': PREAMBLE + PAD.replace('invoke i32', 'invoke fastcc i32'),
            'invoke-bundle': PREAMBLE + PAD.replace('to label %normal', '[ "deopt"(i32 1) ] to label %normal'),
            'invoke-abi': PREAMBLE + PAD.replace('@eh_throw(i32 %x)', '@eh_throw(i32 inreg %x)'),
            'insertvalue-poison': PREAMBLE + PAD.replace('resume {ptr, i32} %lp', '%a = insertvalue {ptr, i32} poison, ptr null, 0\n %b = insertvalue {ptr, i32} %a, i32 0, 1\n resume {ptr, i32} %b'),
            'resume-undef': PREAMBLE + PAD.replace('resume {ptr, i32} %lp', 'resume {ptr, i32} undef'),
            'resume-poison': PREAMBLE + PAD.replace('resume {ptr, i32} %lp', 'resume {ptr, i32} poison'),
            'pair-phi-undef': PREAMBLE + PAD.replace('resume {ptr, i32} %lp', 'br label %merge\nmerge:\n %p = phi {ptr, i32} [undef, %pad]\n resume {ptr, i32} %p'),
            'pair-phi-cycle': PREAMBLE + PAD.replace('resume {ptr, i32} %lp', 'br label %merge\nmerge:\n %p = phi {ptr, i32} [%lp, %pad], [%p, %merge]\n br label %merge'),
            'resume-prototype': PREAMBLE + 'declare i32 @_Unwind_Resume(ptr)\n' + PAD,
            'typeid-not-caught': PREAMBLE + '@_ZTIi = external constant ptr\ndeclare i32 @llvm.eh.typeid.for.p0(ptr)\n' + PAD.replace('resume {ptr, i32} %lp', '%t = call i32 @llvm.eh.typeid.for.p0(ptr @_ZTIi)\n resume {ptr, i32} %lp'),
            'defined-rtti-undef': PREAMBLE + '@_ZTIi = constant ptr undef\n' + PAD.replace('cleanup\n', 'catch ptr @_ZTIi\n'),
            'weak-rtti': PREAMBLE + '@_ZTIi = weak constant ptr null\n' + PAD.replace('cleanup\n', 'catch ptr @_ZTIi\n'),
            'windows-funclet': '''declare i32 @__CxxFrameHandler3(...)
 define void @bad() personality ptr @__CxxFrameHandler3 {
 entry:
  invoke void @g() to label %done unwind label %cs
 cs:
  %s = catchswitch within none [label %cp] unwind to caller
 cp:
  %p = catchpad within %s [ptr null, i32 64, ptr null]
  catchret from %p to label %done
 done:
  ret void
 }
 declare void @g()
''',
            'too-many-types': PREAMBLE + ''.join(f'@T{n} = external constant ptr\n' for n in range(64)) + PAD.replace('cleanup\n', ''.join(f'catch ptr @T{n}\n' for n in range(64))),
            'malformed-llvm': 'define i32 @bad() { ret ptr null }\n',
        }
        for name, body in negative.items():
            path = out/f'reject-{name}.ll'
            path.write_text(HEADER + 'define i32 @good() { ret i32 7 }\n' + body)
            dest = out/'preserved.keel'
            dest.write_text('sentinel\n')
            r = import_(path, dest, False)
            assert r.returncode == 1 and not r.stdout and b'llvm-import:' in r.stderr, (name, r)
            assert dest.read_text() == 'sentinel\n', name
            r = import_(path, success=False)
            assert r.returncode == 1 and not r.stdout, name
            absent = out/f'absent-{name}.keel'
            r = import_(path, absent, False)
            assert r.returncode == 1 and not r.stdout and not absent.exists(), name
            checks.append(dict(kind='rejection-preservation', case=name, stderr=r.stderr.decode(errors='replace')))
        for verifier in ('/usr/bin/false', out/'missing-keel'):
            dest = out/'preserved.keel'
            dest.write_text('sentinel\n')
            r = import_(source, dest, False, keel=verifier)
            assert r.returncode == 1 and not r.stdout and dest.read_text() == 'sentinel\n'
            r = import_(source, success=False, keel=verifier)
            assert r.returncode == 1 and not r.stdout
            absent = out/f'absent-verifier-{Path(verifier).name}.keel'
            r = import_(source, absent, False, keel=verifier)
            assert r.returncode == 1 and not r.stdout and not absent.exists()
            checks.append(dict(kind='protocol-preservation', case=str(verifier)))
        report['status'] = 'passed'
        print(json.dumps(dict(status='passed', native_runs=13, negative_cases=len(negative), checks=len(checks))))
    finally:
        (out/'validation.json').write_text(json.dumps(report, indent=2) + '\n')


if __name__ == '__main__':
    main()
