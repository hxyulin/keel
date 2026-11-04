#!/usr/bin/env python3
"""Bounded Apple AArch64 two-word aggregate ABI interop.

Untouched Clang -O1/-O2 output for a C pair fixture and a C++ std::string_view
fixture is imported and linked across objects with Clang drivers that assert
direct and callback results. Every other aggregate shape is rejected.
"""
from harness import ROOT, classify_import, main, require

FIXTURES = ROOT / 'suite/ir/llvm_import_aggregate_fixtures'
REJECTIONS = {
    'return': 'define [2 x i64] @bad([2 x i64] %x) { ret [2 x i64] %x }',
    'one-word': 'declare void @bad([1 x i64])',
    'three-word': 'declare void @bad([3 x i64])',
    'struct': 'declare void @bad({i64,i64})',
    'float': 'declare void @bad([2 x double])',
    'narrow': 'declare void @bad([2 x i32])',
    'nested': 'declare void @bad([1 x [2 x i64]])',
    'byval': 'declare void @bad(ptr byval([2 x i64]))',
    'inreg': 'declare void @bad([2 x i64] inreg)',
    'load': 'define i64 @bad(ptr %p) { %x = load [2 x i64], ptr %p\n%r = extractvalue [2 x i64] %x, 0\nret i64 %r }',
    'phi': 'define i64 @bad([2 x i64] %x) { br label %next\nnext: %p = phi [2 x i64] [%x, %0]\n%r = extractvalue [2 x i64] %p, 0\nret i64 %r }',
    'partial-poison': 'declare i64 @g([2 x i64])\ndefine i64 @bad(i64 %x) { %a = insertvalue [2 x i64] poison, i64 %x, 0\n%r = call i64 @g([2 x i64] %a)\nret i64 %r }',
    'partial-undef': 'declare i64 @g([2 x i64])\ndefine i64 @bad(i64 %x) { %a = insertvalue [2 x i64] undef, i64 %x, 1\n%r = call i64 @g([2 x i64] %a)\nret i64 %r }',
    'poison-extract': 'define i64 @bad() { %r = extractvalue [2 x i64] poison, 0\nret i64 %r }',
    'constant-aggregate': 'declare i64 @g([2 x i64])\ndefine i64 @bad() { %r = call i64 @g([2 x i64] [i64 1, i64 2])\nret i64 %r }',
    'call-return': 'declare [2 x i64] @g()\ndefine i64 @bad() { %a = call [2 x i64] @g()\n%r = extractvalue [2 x i64] %a, 0\nret i64 %r }',
    'call-contract': 'declare i64 @g([2 x i64])\ndefine i64 @bad([2 x i64] %a) { %r = call i64 @g([2 x i64] inreg %a)\nret i64 %r }',
    'late-function': 'define i64 @good([2 x i64] %a) { %r = extractvalue [2 x i64] %a, 1\nret i64 %r }\ndefine i128 @bad(i128 %x) { ret i128 %x }',
}
CASES = [
    # name, compiler, fixture, driver, flags, expected output
    ('pair', 'clang', 'pair.c', 'driver.c', [], b'1408 aggregate boundary patterns passed\n'),
    ('view', 'clang++', 'view.cpp', 'view_driver.cpp', ['-std=c++17'], b'22 string_view patterns passed\n'),
]


def validate(ctx):
    run, out = ctx.run, ctx.out
    for name, compiler, fixture, driver, flags, message in CASES:
        compiler = {'clang': ctx.clang, 'clang++': ctx.clangxx}[compiler]
        driver_obj = out / f'{name}-driver.o'
        run.require([compiler, *flags, '-O2', '-c', FIXTURES / driver, '-o', driver_obj])
        for opt in ('O1', 'O2'):
            stem = f'{name}-{opt}'
            ll = out / f'{stem}.ll'
            run.require([compiler, *flags, '-' + opt, '-S', '-emit-llvm', FIXTURES / fixture, '-o', ll])
            run.require([compiler, '-c', ll, '-o', out / f'{stem}.reference.o'])
            expected = ctx.link_and_run(compiler, [driver_obj, out / f'{stem}.reference.o'],
                                        out / f'{stem}.reference')
            require(expected == message, f'{stem}: reference oracle failed')
            text = out / f'{stem}.keel'
            ctx.importer.accept(ll, text)
            ctx.tiers(stem, text, [driver_obj], expected, compiler=compiler)
    for name, body in REJECTIONS.items():
        diagnostic = ctx.importer.reject(ctx.module(name, body))
        require(classify_import(1, b'', diagnostic, True) == 'rejected',
                f'{name}: not a known feature rejection: {diagnostic}')
        ctx.check(kind='rejection', case=name, diagnostic=diagnostic.strip())


if __name__ == '__main__':
    raise SystemExit(main(validate, __doc__))
