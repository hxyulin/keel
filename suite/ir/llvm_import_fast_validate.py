#!/usr/bin/env python3
"""Private LLVM fastcc functions.

The imported text must match the golden llvm_import_fast_fixtures/fast.keel,
which keel-codegen's fast_cc test also compiles. The output is built for both
targets with every tier, selector, allocator and emission route; AArch64
objects also run against the Clang reference. Exported, declared or
address-taken fastcc functions are rejected.
"""
from harness import ROOT, TIERS, main, require

FIXTURES = ROOT / 'suite/ir/llvm_import_fast_fixtures'
EXPECTED = b'516 private fastcc patterns passed\n'
REJECTIONS = {
    'signed-boolean': 'define private fastcc signext i1 @bad() { ret i1 true }',
    'external': 'define fastcc i64 @bad(i64 %x) { ret i64 %x }',
    'declaration': 'declare fastcc i64 @bad(i64)',
    'escaping-address': 'define internal fastcc i64 @bad(i64 %x) { ret i64 %x }\n@escaped = global ptr @bad',
    'mismatched-call': 'define internal fastcc i64 @bad(i64 %x) { ret i64 %x }\ndefine i64 @f(i64 %x) { %r = call i64 @bad(i64 %x)\nret i64 %r }',
    'escaping-argument': 'define internal fastcc void @bad(ptr %x) { ret void }\ndefine void @f() { call fastcc void @bad(ptr @bad)\nret void }',
}


def validate(ctx):
    run, out, keel = ctx.run, ctx.out, ctx.keel
    source = FIXTURES / 'fast.ll'
    text = ctx.importer.accept(source, out / 'fast.keel')
    require(text == (FIXTURES / 'fast.keel').read_text(), 'imported fast fixture changed; review the golden')
    require(text.count('cc(fast) noinline local') == 4, 'fast convention translation structure')
    run.require([ctx.clang, '-O2', '-c', FIXTURES / 'driver.c', '-o', out / 'driver.o'])
    run.require([ctx.clang, '-c', source, '-o', out / 'reference.o'])
    require(ctx.link_and_run(ctx.clang, [out / 'driver.o', out / 'reference.o'], out / 'reference') == EXPECTED,
            'reference oracle')
    for target in ('aarch64-apple-darwin', 'x86_64-unknown-linux-gnu'):
        for tier in TIERS:
            for selector in ('baseline', 'full'):
                for allocator in ('fast', 'full'):
                    for route in ('object', 'text'):
                        stem = out / '-'.join([target, tier, selector, allocator, route])
                        obj = stem.with_suffix('.o')
                        flags = ['--target', target, '--verify-each', '--isel-tier', selector,
                                 '--regalloc', allocator]
                        if route == 'object':
                            ctx.compile(out / 'fast.keel', obj, tier, *flags)
                        else:
                            asm = stem.with_suffix('.s')
                            run.require([keel, 'compile', *flags, '--opt-tier', tier, out / 'fast.keel', '-o', asm])
                            run.require([keel, 'asm', '--target', target, asm, '-o', obj])
                        native = target == 'aarch64-apple-darwin'
                        if native:
                            require(ctx.link_and_run(ctx.clang, [out / 'driver.o', obj], stem) == EXPECTED,
                                    stem.name)
                        ctx.check(kind='compile', target=target, tier=tier, selector=selector,
                                  allocator=allocator, route=route, native=native)
    header = source.read_text().split('declare i64')[0]
    markers = dict.fromkeys(REJECTIONS, ('fastcc requires', 'fastcc function address escapes'))
    markers['signed-boolean'] = 'i1 ABI'
    ctx.reject_all(REJECTIONS, header, markers=markers)


if __name__ == '__main__':
    raise SystemExit(main(validate, __doc__))
