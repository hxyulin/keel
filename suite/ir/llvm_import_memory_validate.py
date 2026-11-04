#!/usr/bin/env python3
"""Bounded LLVM memcpy, memmove and memset intrinsics.

Imports a handwritten module and runs it at every tier through both the object
and the assembly text route. Volatile, i32-length, inline and address-taken
intrinsics are rejected.
"""
from harness import ROOT, TIERS, main, require

FIXTURES = ROOT / 'suite/ir/llvm_import_memory_fixtures'
EXPECTED = b'memory intrinsic oracle passed\n'


def validate(ctx):
    run, out, keel = ctx.run, ctx.out, ctx.keel
    source = FIXTURES / 'memory.ll'
    text = ctx.importer.accept(source, out / 'memory.keel')
    require('decl @llvm.mem' not in text and all('intrinsic ' + n in text for n in ('memmove', 'memset')),
            'intrinsic translation structure')
    run.require([ctx.clang, '-O2', '-c', FIXTURES / 'driver.c', '-o', out / 'driver.o'])
    run.require([ctx.clang, '-c', source, '-o', out / 'reference.o'])
    require(ctx.link_and_run(ctx.clang, [out / 'driver.o', out / 'reference.o'], out / 'reference') == EXPECTED,
            'reference oracle')
    for tier in TIERS:
        for route in ('object', 'text'):
            stem = out / f'{tier}-{route}'
            obj = stem.with_suffix('.o')
            if route == 'object':
                ctx.compile(out / 'memory.keel', obj, tier, '--verify-each')
            else:
                asm = stem.with_suffix('.s')
                run.require([keel, 'compile', '--verify-each', '--opt-tier', tier, out / 'memory.keel', '-o', asm])
                run.require([keel, 'asm', asm, '-o', obj])
            require(ctx.link_and_run(ctx.clang, [out / 'driver.o', obj], stem) == EXPECTED, (tier, route))
            ctx.check(kind='native', tier=tier, route=route)
    original = source.read_text()
    rejections = {
        'volatile': original.replace('i1 false', 'i1 true'),
        'i32-length': original.replace('.i64', '.i32').replace('i64', 'i32'),
        'inline': original.replace('llvm.memcpy.p0.p0', 'llvm.memcpy.inline.p0.p0'),
        'ordinary-i1': original + '\ndefine i1 @bad() { ret i1 true }\n',
        # The LLVM verifier rejects this one; it is not importer feature evidence.
        'address': original + '\n@escaped = global ptr @llvm.memcpy.p0.p0.i64\n',
    }
    ctx.reject_all(rejections, header='', markers={
        'volatile': 'volatile memory intrinsics',
        'i32-length': 'unsupported parameter/return ABI attribute',
        'inline': 'unsupported parameter/return ABI attribute',
        'ordinary-i1': 'i1 ABI',
        'address': 'Invalid user of intrinsic instruction',
    })


if __name__ == '__main__':
    raise SystemExit(main(validate, __doc__))
