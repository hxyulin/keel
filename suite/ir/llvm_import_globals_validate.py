#!/usr/bin/env python3
"""Global objects, nested layouts and floating-point initializers.

Clang -O0 C and C++ output and two handwritten modules are imported verbatim
and checked by native drivers that assert every object's contents. The
rejection cases in llvm_import_globals_fixtures/rejections.json are valid LLVM
modules, so they exercise importer guards rather than parse errors.
"""
import json

from harness import ROOT, main, require

FIXTURES = ROOT / 'suite/ir/llvm_import_globals_fixtures'
CASES = [
    ('clang-c', 'clang', FIXTURES / 'objects.c', FIXTURES / 'driver.c', b'global object assertions passed\n'),
    ('clang-cpp', 'clang++', FIXTURES / 'objects.cpp', FIXTURES / 'driver.c', b'global object assertions passed\n'),
    ('layout', None, FIXTURES / 'layout.ll', FIXTURES / 'layout_driver.c', b'nested layout assertions passed\n'),
    ('fp', None, FIXTURES / 'fp.ll', FIXTURES / 'fp_driver.c', b'floating global assertions passed\n'),
]


def validate(ctx):
    run, out = ctx.run, ctx.out
    for name, body in json.loads((FIXTURES / 'rejections.json').read_text()).items():
        diagnostic = ctx.importer.reject(ctx.module(name, body))
        require('input.ll:' not in diagnostic and 'input verification' not in diagnostic,
                f'{name}: parse or verifier error instead of an importer guard: {diagnostic}')
        ctx.check(kind='rejection', case=name, diagnostic=diagnostic.strip())
    for name, compiler, source, driver, message in CASES:
        if compiler:
            emitted = out / f'{name}.ll'
            run.require([{'clang': ctx.clang, 'clang++': ctx.clangxx}[compiler], '-O0', '-fno-common',
                         '-S', '-emit-llvm', source, '-o', emitted])
            source = emitted
        # Even a failure in the final Keel verifier must preserve output.
        ctx.verifier_failures(source)
        text = out / f'{name}.keel'
        ctx.importer.accept(source, text)
        run.require([ctx.clang, '-c', source, '-o', out / f'{name}.reference.o'])
        run.require([ctx.clang, '-O0', '-c', driver, '-o', out / f'{name}.driver.o'])
        expected = ctx.link_and_run(ctx.clang, [out / f'{name}.driver.o', out / f'{name}.reference.o'],
                                    out / f'{name}.reference')
        require(expected == message, (name, expected))
        ctx.tiers(name, text, [out / f'{name}.driver.o'], expected)


if __name__ == '__main__':
    raise SystemExit(main(validate, __doc__, timeout=90))
