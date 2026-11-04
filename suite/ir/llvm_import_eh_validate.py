#!/usr/bin/env python3
"""Bounded Itanium exception handling with unmodified Clang++ IR.

Imports Clang++ catch/cleanup/rethrow code, handwritten landing-pad edges and a
checked-in Clang snapshot, and runs them against external native throwing
functions at every tier. Unsupported EH forms are rejected without output.
"""
from harness import ROOT, TIERS, main, require

FIXTURES = ROOT / 'suite/ir/llvm_import_eh_fixtures'
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
REJECTIONS = {
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


def validate(ctx):
    run, out, clang = ctx.run, ctx.out, ctx.clangxx
    run.require([clang, '--version'])
    catches = out / 'catches.ll'
    run.require([clang, '-O1', '-fwrapv', '-Xclang', '-disable-lifetime-markers',
                 '-S', '-emit-llvm', FIXTURES / 'catches.cpp', '-o', catches])
    sources = dict(catches=catches, edges=FIXTURES / 'edges.ll')
    run.require([clang, '-O2', '-c', FIXTURES / 'driver.cpp', '-o', out / 'driver.o'])
    texts = {}
    for name, path in sources.items():
        texts[name] = ctx.importer.accept(path, out / f'{name}.keel')
        run.require([clang, '-O1', '-c', path, '-o', out / f'{name}.reference.o'])
    ctx.importer.accept(FIXTURES / 'catches.clang.ll', out / 'snapshot.keel')
    text = texts['catches']
    require(all(f'func @{name}(' in text for name in ('eh_catches', 'eh_typed', 'eh_cleaned', 'eh_rethrow')),
            'catches: missing function')
    require('landingpad catch @_ZTIi, catch null' in text, 'catches: landing pad clauses')
    require('eh_typeid @_ZTIi' in text and 'call @_Unwind_Resume(' in text, 'catches: typeid or resume')
    require('invoke @__cxa_rethrow(' in text, 'catches: rethrow')
    require(texts['edges'].count('landingpad catch null') == 2, 'edges: catch-all landing pads')
    expected = ctx.link_and_run(clang, [out / 'driver.o', *[out / f'{n}.reference.o' for n in sources]],
                                out / 'reference')
    require(expected == b'EH: 1130 independently asserted inputs\n', expected)
    for tier in TIERS:
        for name in sources:
            ctx.compile(out / f'{name}.keel', out / f'{name}.{tier}.o', tier)
        snapshot = ctx.compile(out / 'snapshot.keel', out / f'snapshot.{tier}.o', tier)
        actual = ctx.link_and_run(clang, [out / 'driver.o', snapshot, out / 'edges.reference.o'],
                                  out / f'snapshot.{tier}')
        require(actual == expected, (tier, 'clang-snapshot'))
        ctx.check(kind='native', tier=tier, replacement='clang-snapshot', inputs=1130)
        for replacement in ('catches', 'edges', 'all'):
            objects = [out / f'{n}.{tier if replacement in (n, "all") else "reference"}.o' for n in sources]
            actual = ctx.link_and_run(clang, [out / 'driver.o', *objects], out / f'{replacement}.{tier}')
            require(actual == expected, (tier, replacement))
            ctx.check(kind='native', tier=tier, replacement=replacement, inputs=1130)

    header = (FIXTURES / 'edges.ll').read_text().split('declare')[0] + 'define i32 @good() { ret i32 7 }\n'
    ctx.reject_all(REJECTIONS, header)
    ctx.verifier_failures(catches)


if __name__ == '__main__':
    raise SystemExit(main(validate, __doc__))
