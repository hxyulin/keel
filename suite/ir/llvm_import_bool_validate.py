#!/usr/bin/env python3
"""Apple AArch64 boolean ABI interop.

Clang -O0, -O1 and -O2 output for 1024 boolean argument and return patterns is
imported and checked at every tier. i1 values without a zeroext contract are
rejected.
"""
from harness import ROOT, main, require

FIXTURES = ROOT / 'suite/ir/llvm_import_bool_fixtures'
REJECTIONS = {
    'plain-return': 'define i1 @bad() { ret i1 true }',
    'plain-argument': 'define i32 @bad(i1 %x) { %r = zext i1 %x to i32\nret i32 %r }',
    'signed-return': 'define signext i1 @bad() { ret i1 true }',
    'signed-argument': 'define i32 @bad(i1 signext %x) { %r = zext i1 %x to i32\nret i32 %r }',
    'call-mismatch': 'declare zeroext i1 @g(i1 zeroext)\ndefine zeroext i1 @bad(i1 zeroext %x) { %r = call signext i1 @g(i1 zeroext %x)\nret i1 %r }',
    'indirect-plain': 'define i32 @bad(ptr %f) { %r = call i1 %f()\n%z = zext i1 %r to i32\nret i32 %z }',
}


def validate(ctx):
    run, out = ctx.run, ctx.out
    run.require([ctx.clang, '-O2', '-c', FIXTURES / 'driver.c', '-o', out / 'driver.o'])
    for opt in ('O0', 'O1', 'O2'):
        ll = out / f'bool-{opt}.ll'
        run.require([ctx.clang, '-' + opt, '-S', '-emit-llvm', FIXTURES / 'bool.c', '-o', ll])
        run.require([ctx.clang, '-c', ll, '-o', out / f'{opt}.reference.o'])
        expected = ctx.link_and_run(ctx.clang, [out / 'driver.o', out / f'{opt}.reference.o'],
                                    out / f'{opt}.reference')
        require(expected == b'1024 boolean patterns passed\n', 'reference oracle failed')
        text = out / f'bool-{opt}.keel'
        ctx.importer.accept(ll, text)
        ctx.tiers(f'bool-{opt}', text, [out / 'driver.o'], expected, patterns=1024)
    ctx.reject_all(REJECTIONS, markers=dict.fromkeys(REJECTIONS, 'i1 ABI'))


if __name__ == '__main__':
    raise SystemExit(main(validate, __doc__))
