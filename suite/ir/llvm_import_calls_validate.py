#!/usr/bin/env python3
"""Indirect calls, callbacks and the sret aggregate ABI.

Clang-emitted LLVM is imported byte for byte. Each imported object replaces its
Clang-compiled counterpart, one at a time and all together, so native callers
call Keel code and Keel code calls native callbacks. Unsupported call contracts
are rejected without output.

Supported aggregate contract: first ptr sret of unpacked {i64,i64,i64}, void
result, size 24, align 8, offsets 0/8/16, passed in x8. Larger integer layouts,
small and HFA sret, byval and aggregate SSA values are not supported yet.
"""
from harness import ROOT, TIERS, main, require

FIXTURES = ROOT / 'suite/ir/llvm_import_calls_fixtures'
REJECTIONS = {
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
    'unknown-call-contract': 'define i32 @bad(ptr %f) { %r = call i32 %f() "future-call-abi"\nret i32 %r }',
    'returns-twice': 'define i32 @bad(ptr %f) { %r = call i32 %f() returns_twice\nret i32 %r }',
    'aggregate-ssa': 'define {i64,i64} @bad() { ret {i64,i64} zeroinitializer }',
}


def validate(ctx):
    run, out = ctx.run, ctx.out
    flags = ['-O1', '-fwrapv', '-Xclang', '-disable-lifetime-markers']
    sources = {}
    for name in ('calls', 'aggregate'):
        sources[name] = out / f'{name}.ll'
        run.require([ctx.clang, *flags, '-S', '-emit-llvm', FIXTURES / f'{name}.c', '-o', sources[name]])
    sources['phi'] = FIXTURES / 'phi.ll'
    run.require([ctx.clang, '-O2', '-c', FIXTURES / 'driver.c', '-o', out / 'driver.o'])
    imported = {}
    for name, source in sources.items():
        imported[name] = ctx.importer.accept(source, out / f'{name}.keel')
        run.require([ctx.clang, '-c', source, '-o', out / f'{name}.reference.o'])
    require('call_indirect' in imported['calls'], 'calls: no call_indirect')
    require('symbol_addr @external_add' in imported['calls'], 'calls: no callback address')
    require('ptr sret {i64, i64, i64}' in imported['aggregate'], 'aggregate: no sret parameter')
    expected = ctx.link_and_run(ctx.clang, [out / 'driver.o', *[out / f'{n}.reference.o' for n in sources]],
                                out / 'reference')
    require(b'2049 independently asserted inputs' in expected, expected)
    for tier in TIERS:
        for name in sources:
            ctx.compile(out / f'{name}.keel', out / f'{name}.{tier}.o', tier)
        for replacement in ('calls', 'aggregate', 'phi', 'all'):
            objects = [out / f'{n}.{tier if replacement in (n, "all") else "reference"}.o' for n in sources]
            actual = ctx.link_and_run(ctx.clang, [out / 'driver.o', *objects], out / f'{replacement}.{tier}')
            require(actual == expected, (tier, replacement))
            ctx.check(kind='native', tier=tier, replacement=replacement, inputs=2049)

    # A good function before the rejected one checks whole-module buffering.
    header = (FIXTURES / 'phi.ll').read_text().split('declare')[0] + 'define i32 @good() { ret i32 7 }\n'
    ctx.reject_all(REJECTIONS, header)
    ctx.verifier_failures(sources['calls'])


if __name__ == '__main__':
    raise SystemExit(main(validate, __doc__))
